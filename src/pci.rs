//! PCI bus enumeration.
//!
//! Brute-forces every bus/slot/function through the legacy config-space
//! mechanism, logs each device found, and hands AHCI controllers to the disk
//! driver. Enumeration is unchanged from the original; only the language is.

use crate::io::{inl, outl};
use crate::kpci;

#[cfg(feature = "ahci")]
use crate::{ahci, mm};

#[cfg(feature = "ahci")]
const ABAR_MMIO: u64 = 0xFFFF_B000_0000_0000;

#[inline]
pub fn config_read32(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let address = (1u32 << 31)
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset & 0xFC) as u32);

    unsafe {
        outl(0xCF8, address);
        inl(0xCFC)
    }
}

#[inline]
fn dev_check(id: u32) -> bool {
    (id & 0xFFFF) != 0xFFFF
}

#[inline]
unsafe fn read_mmio32(address: u64) -> u32 {
    core::ptr::read_volatile(address as *const u32)
}

pub fn enumerate() {
    for bus in 0u16..256 {
        for slot in 0u8..32 {
            for func in 0u8..8 {
                let id = config_read32(bus as u8, slot, func, 0x00);
                if !dev_check(id) {
                    continue;
                }

                let class_info = config_read32(bus as u8, slot, func, 0x08);
                let revision = class_info & 0xFF;
                // Only the (feature-gated) log line reads `revision`.
                let _ = revision;
                let prog_if = (class_info >> 8) & 0xFF;
                let subclass = (class_info >> 16) & 0xFF;
                let class = (class_info >> 24) & 0xFF;

                kpci!(
                    "PCI {:02X}:{:02X}.{:X} class={:02X} subclass={:02X} prog-if={:02X} rev={:02X}\n",
                    bus as u8,
                    slot,
                    func,
                    class,
                    subclass,
                    prog_if,
                    revision
                );

                // Mass storage controller / SATA / AHCI 1.0.
                if class != 0x01 || subclass != 0x06 || prog_if != 0x01 {
                    continue;
                }

                #[cfg(feature = "ahci")]
                {
                    // BAR5 contains the AHCI ABAR. A 32-bit MMIO BAR keeps its
                    // flags in the low four bits.
                    let bar5 = config_read32(bus as u8, slot, func, 0x24);
                    let abar = bar5 & !0xF;

                    // The register file is 0x2000 bytes: the HBA occupies the
                    // first page and the 32 port blocks run from 0x100 to
                    // 0x2000, so both pages are mapped.
                    mm::vmm_map_mmio(ABAR_MMIO, abar as u64);
                    mm::vmm_map_mmio(ABAR_MMIO + 0x1000, abar as u64 + 0x1000);

                    kpci!("AHCI BAR5 = 0x{:08X}\n", abar);

                    let cap = unsafe { read_mmio32(ABAR_MMIO + 0x00) };
                    let port_count = (cap & 0x1F) + 1;

                    kpci!("AHCI CAP = 0x{:08X}\n", cap);
                    kpci!("AHCI ports supported = {}\n", port_count);

                    let pi = unsafe { read_mmio32(ABAR_MMIO + 0x0C) };
                    kpci!("AHCI PI = 0x{:08X}\n", pi);

                    for port in 0..port_count {
                        if pi & (1u32 << port) == 0 {
                            continue;
                        }

                        let port_base = ABAR_MMIO + 0x100 + (port as u64 * 0x80);

                        let ssts = unsafe { read_mmio32(port_base + 0x28) };
                        let det = ssts & 0xF;
                        let ipm = (ssts >> 8) & 0xF;
                        let pxsig = unsafe { read_mmio32(port_base + 0x24) };

                        kpci!(
                            "port {}: SSTS=0x{:08X} DET={} IPM={} SIG=0x{:08X}\n",
                            port,
                            ssts,
                            det,
                            ipm,
                            pxsig
                        );

                        // DET = 3: device present, communication established.
                        // IPM = 1: interface in the active power-management state.
                        if det == 3 && ipm == 1 {
                            if pxsig == 0x0000_0101 {
                                kpci!("SATA ATA device detected on port {}\n", port);
                                // ABAR_MMIO, not abar: abar is physical.
                                ahci::init(ABAR_MMIO, port as u8);
                            } else if pxsig == 0xEB14_0101 {
                                kpci!("SATA ATAPI device detected on port {}\n", port);
                            } else {
                                kpci!(
                                    "Unknown SATA device signature on port {}: 0x{:08X}\n",
                                    port,
                                    pxsig
                                );
                            }
                        }
                    }
                }

                #[cfg(not(feature = "ahci"))]
                {
                    // Said out loud rather than dropped silently: a boot log
                    // with no AHCI lines is indistinguishable from a machine
                    // that has no SATA controller.
                    kpci!(
                        "AHCI controller at {:02X}:{:02X}.{:X} detected, setup compiled out (--no-default-features)\n",
                        bus as u8,
                        slot,
                        func
                    );
                }
            }
        }
    }
}
