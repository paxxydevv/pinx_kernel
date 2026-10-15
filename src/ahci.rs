//! AHCI host controller driver.
//!
//! Each implemented port gets a command list, FIS receive area and command
//! table, mapped with their physical addresses handed to the HBA. A fixed
//! DMA staging window shared by all ports backs every data transfer through
//! chained PRDT entries, so any sector-aligned, multi-sector request up to
//! the window size is issued as a single command:
//!
//!   * READ DMA EXT (0x25) / WRITE DMA EXT (0x35) for data,
//!   * IDENTIFY DEVICE (0xEC) to size and record a drive,
//!   * FLUSH CACHE EXT (0xEA) to commit writes.
//!
//! Drives are addressed by an index into a registry built by `init`; the
//! caller sees only that index plus a 48-bit LBA.
//!
//! This module is only compiled when the `ahci` feature is on.

use crate::{kerror, kinfo, kpanic, mm};

const AHCI_PCMD_ST: u32 = 1 << 0;
const AHCI_PCMD_FRE: u32 = 1 << 4;
const AHCI_PCMD_CR: u32 = 1 << 15;
const AHCI_GHC_AE: u32 = 1 << 31;
const AHCI_PCMD_FR: u32 = 1 << 14;
const AHCI_CL_CFL_5: u16 = 5;
const AHCI_CL_CBR: u16 = 1 << 10;

const AHCI_PORT_MAX: usize = 32;

const AHCI_WINDOW_BASE: u64 = 0xFFFF_C000_0000_0000;
const AHCI_WINDOW_SIZE: u64 = 0x3000;
const AHCI_CL_OFF: u64 = 0x0000;
const AHCI_FB_OFF: u64 = 0x1000;
const AHCI_CT_OFF: u64 = 0x2000;
const AHCI_PAGE_SIZE: u64 = 0x1000;

// DMA staging window: 64 pages, contiguous in the virtual address space.
const AHCI_DMA_PAGES: usize = 64;
const AHCI_DMA_BASE: u64 = 0xFFFF_C100_0000_0000;
const AHCI_MAX_TRANSFER: usize = AHCI_DMA_PAGES * 0x1000;

const AHCI_CR_SPINS: u64 = 1_000_000;
const AHCI_FR_SPINS: u64 = 1_000_000;
const AHCI_SECTOR_SIZE: usize = 512;
const AHCI_SECTOR_MAX: usize = AHCI_MAX_TRANSFER / AHCI_SECTOR_SIZE;

const AHCI_FIS_TYPE_REG_H2D: u8 = 0x27;
const AHCI_CMD_READ_DMA_EXT: u8 = 0x25;
const AHCI_CMD_WRITE_DMA_EXT: u8 = 0x35;
const AHCI_CMD_IDENTIFY_DEVICE: u8 = 0xEC;
const AHCI_CMD_FLUSH_CACHE_EXT: u8 = 0xEA;

const AHCI_TFD_BSY: u32 = 1 << 7;
const AHCI_TFD_DRQ: u32 = 1 << 3;
const AHCI_PXIS_TFES: u32 = 1 << 30;

const AHCI_S64A: u32 = 1 << 31;
const AHCI_XFER_SPINS: u64 = 20_000_000;

struct AhciDevice {
    present: bool,
    hba: u64,
    port: u8,
    sectors: u64,
    lba48: bool,
}

static mut AHCI_DEVICES: [AhciDevice; AHCI_PORT_MAX] = {
    const EMPTY: AhciDevice = AhciDevice {
        present: false,
        hba: 0,
        port: 0,
        sectors: 0,
        lba48: false,
    };
    [EMPTY; AHCI_PORT_MAX]
};

static mut AHCI_DEVICE_COUNT: usize = 0;
static mut AHCI_DMA_READY: bool = false;
static mut AHCI_DMA_PHYS: [u64; AHCI_DMA_PAGES] = [0; AHCI_DMA_PAGES];

#[repr(C)]
struct AhciPort {
    clb: u64,
    fb: u64,
    is: u32,
    ie: u32,
    cmd: u32,
    reserved0: u32,
    tfd: u32,
    sig: u32,
    ssts: u32,
    sctl: u32,
    serr: u32,
    sact: u32,
    ci: u32,
    sntf: u32,
    fbs: u32,
    devslp: u32,
    reserved1: [u8; 0x70 - 0x48],
    vendor: [u8; 0x80 - 0x70],
}

#[repr(C)]
struct AhciHbaMem {
    cap: u32,
    ghc: u32,
    is: u32,
    pi: u32,
    vs: u32,
    ccc_ctl: u32,
    ccc_pts: u32,
    em_loc: u32,
    em_ctl: u32,
    cap2: u32,
    bohc: u32,
    reserved: [u8; 0xA0 - 0x2C],
    vendor: [u8; 0x100 - 0xA0],
    ports: [AhciPort; AHCI_PORT_MAX],
}

#[repr(C)]
struct AhciCmdHeader {
    flags: u16,
    prdtl: u16,
    prdbc: u32,
    ctba: u32,
    ctbau: u32,
    reserved: [u32; 4],
}

#[repr(C)]
struct AhciFisRegH2d {
    fis_type: u8,
    flags: u8,
    command: u8,
    featurel: u8,
    lba0: u8,
    lba1: u8,
    lba2: u8,
    device: u8,
    lba3: u8,
    lba4: u8,
    lba5: u8,
    featureh: u8,
    countl: u8,
    counth: u8,
    icc: u8,
    control: u8,
    reserved: [u8; 4],
}

#[repr(C)]
struct AhciPrdtEntry {
    dba: u32,
    dbau: u32,
    reserved: u32,
    dbc: u32,
}

const _: () = assert!(core::mem::offset_of!(AhciHbaMem, ports) == 0x100);
const _: () = assert!(core::mem::size_of::<AhciPort>() == 0x80);
const _: () = assert!(core::mem::size_of::<AhciCmdHeader>() == 32);
const _: () = assert!(core::mem::size_of::<AhciFisRegH2d>() == 20);

fn map_page(virtual_address: u64) -> u64 {
    let physical = mm::pmm_alloc_page();
    if physical != mm::PMM_NO_PAGE {
        mm::vmm_map_page(virtual_address, physical);
    }
    physical
}

fn reset_page(address: u64) {
    unsafe {
        let words = address as *mut u32;
        for i in 0..(AHCI_PAGE_SIZE / 4) {
            core::ptr::write_volatile(words.add(i as usize), 0);
        }

        let mut offset = 0;
        while offset < AHCI_PAGE_SIZE {
            core::arch::asm!("clflush [{}]", in(reg) address + offset, options(nostack));
            offset += 64;
        }

        core::arch::asm!("sfence", options(nostack));
    }
}

/// Allocate and identity-map the shared DMA staging window. Called once, the
/// first time a port comes up.
fn dma_window_init() {
    if unsafe { AHCI_DMA_READY } {
        return;
    }

    for i in 0..AHCI_DMA_PAGES {
        let virtual_address = AHCI_DMA_BASE + (i as u64 * AHCI_PAGE_SIZE);
        let physical = map_page(virtual_address);
        if physical == mm::PMM_NO_PAGE {
            kpanic!("ahci: cannot map DMA page {}\n", i);
        }
        unsafe {
            AHCI_DMA_PHYS[i] = physical;
        }
    }

    unsafe {
        AHCI_DMA_READY = true;
    }

    kinfo!("ahci: DMA window ready ({} KiB)\n", AHCI_MAX_TRANSFER / 1024);
}

fn dma_virt() -> u64 {
    AHCI_DMA_BASE
}

/// Run one DMA command on `device`. `sectors` bytes of data move through the
/// staging window; when `to_disk` is true the contents of `buf` are staged
/// first (write), otherwise they are copied out after completion (read).
/// `lba` is always encoded as a 48-bit LBA.
unsafe fn xfer(device: &AhciDevice, command: u8, lba: u64, buf: *const u8, len: usize, to_disk: bool) -> bool {
    if len == 0 || len % AHCI_SECTOR_SIZE != 0 || len > AHCI_MAX_TRANSFER {
        kerror!("ahci: xfer: bad length {}\n", len);
        return false;
    }

    let sectors = len / AHCI_SECTOR_SIZE;
    if sectors > AHCI_SECTOR_MAX as usize {
        kerror!("ahci: xfer: too many sectors\n");
        return false;
    }

    let hba_ptr = device.hba as *mut AhciHbaMem;
    let port = core::ptr::addr_of_mut!((*hba_ptr).ports[device.port as usize]);

    let dma = dma_virt();
    reset_page(dma);

    if to_disk {
        core::ptr::copy_nonoverlapping(buf, dma as *mut u8, len);
    }

    let window = AHCI_WINDOW_BASE + device.port as u64 * AHCI_WINDOW_SIZE;
    let command_list = window + AHCI_CL_OFF;
    let command_table = window + AHCI_CT_OFF;

    // Wait until the port is ready to accept a command.
    let mut ready = false;
    for _ in 0..AHCI_XFER_SPINS {
        let tfd = core::ptr::read_volatile(core::ptr::addr_of!((*port).tfd));
        if tfd & (AHCI_TFD_BSY | AHCI_TFD_DRQ) == 0 {
            ready = true;
            break;
        }
        core::arch::asm!("pause", options(nomem, nostack));
    }
    if !ready {
        kerror!("ahci: port {} stayed busy\n", device.port);
        return false;
    }

    // Slot 0 must be free before its header/table are rewritten.
    let ci = core::ptr::read_volatile(core::ptr::addr_of!((*port).ci));
    if ci & 1 != 0 {
        kerror!("ahci: port {} command slot 0 is busy\n", device.port);
        return false;
    }

    // Clear stale port interrupt and error status.
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*port).is), 0xFFFF_FFFF);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*port).serr), 0xFFFF_FFFF);

    let prdtl = (len + (AHCI_PAGE_SIZE as usize) - 1) / (AHCI_PAGE_SIZE as usize);

    let header = command_list as *mut AhciCmdHeader;
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*header).flags), AHCI_CL_CFL_5 | AHCI_CL_CBR);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*header).prdtl), prdtl as u16);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*header).prdbc), 0);

    let prdt = (command_table + 0x80) as *mut AhciPrdtEntry;
    for i in 0..prdtl {
        let entry = prdt.add(i);
        let phys = unsafe { AHCI_DMA_PHYS[i] };
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*entry).dba), phys as u32);
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*entry).dbau), (phys >> 32) as u32);
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*entry).reserved), 0);
        let remaining = len - (i * AHCI_PAGE_SIZE as usize);
        let bytes = if remaining > AHCI_PAGE_SIZE as usize {
            AHCI_PAGE_SIZE as usize
        } else {
            remaining
        };
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*entry).dbc), (bytes - 1) as u32);
    }

    let fis = command_table as *mut AhciFisRegH2d;
    core::ptr::write_bytes(fis, 0, 1);

    core::ptr::write_volatile(core::ptr::addr_of_mut!((*fis).fis_type), AHCI_FIS_TYPE_REG_H2D);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*fis).flags), 1 << 7);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*fis).command), command);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*fis).lba0), lba as u8);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*fis).lba1), (lba >> 8) as u8);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*fis).lba2), (lba >> 16) as u8);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*fis).lba3), (lba >> 24) as u8);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*fis).lba4), (lba >> 32) as u8);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*fis).lba5), (lba >> 40) as u8);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*fis).device), 1 << 6);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*fis).countl), sectors as u8);
    core::ptr::write_volatile(core::ptr::addr_of_mut!((*fis).counth), (sectors >> 8) as u8);

    core::arch::asm!("mfence", options(nostack));

    core::ptr::write_volatile(core::ptr::addr_of_mut!((*port).ci), 1);

    let mut completed = false;
    for _ in 0..AHCI_XFER_SPINS {
        let current_ci = core::ptr::read_volatile(core::ptr::addr_of!((*port).ci));
        if current_ci & 1 == 0 {
            completed = true;
            break;
        }
        core::arch::asm!("pause", options(nomem, nostack));
    }
    if !completed {
        kerror!("ahci: port {} command timed out\n", device.port);
        return false;
    }

    let status = core::ptr::read_volatile(core::ptr::addr_of!((*port).is));
    let tfd = core::ptr::read_volatile(core::ptr::addr_of!((*port).tfd));
    if status & AHCI_PXIS_TFES != 0 || (tfd & 1) != 0 {
        kerror!(
            "ahci: xfer failed, port={}, PxIS=0x{:08X}, PxTFD=0x{:08X}\n",
            device.port,
            status,
            tfd
        );
        return false;
    }

    if !to_disk && buf as usize != dma as usize {
        core::ptr::copy_nonoverlapping(dma as *const u8, buf as *mut u8, len);
    }

    true
}

fn identify(hba: u64, port_num: u8) -> Option<(u64, bool)> {
    let device = AhciDevice {
        present: true,
        hba,
        port: port_num,
        sectors: 0,
        lba48: false,
    };

    unsafe {
        if !xfer(&device, AHCI_CMD_IDENTIFY_DEVICE, 0, dma_virt() as *const u8, AHCI_SECTOR_SIZE, false) {
            return None;
        }
    }

    let ident = unsafe { core::slice::from_raw_parts(dma_virt() as *const u8, 512) };

    let word = |i: usize| -> u16 {
        (ident[i * 2 + 1] as u16) << 8 | ident[i * 2] as u16
    };

    let lba48 = word(83) & (1 << 10) != 0;
    let sectors = if lba48 {
        (word(100) as u64)
            | ((word(101) as u64) << 16)
            | ((word(102) as u64) << 32)
            | ((word(103) as u64) << 48)
    } else {
        word(60) as u64 | ((word(61) as u64) << 16)
    };

    kinfo!(
        "ahci: port {} identified, {} sectors ({}, lba48={})\n",
        port_num,
        sectors,
        if sectors > 0 { "size detected" } else { "size unknown" },
        lba48
    );

    if sectors == 0 {
        None
    } else {
        Some((sectors, lba48))
    }
}

/// Bring up one port and register its drive if IDENTIFY succeeds.
pub fn init(hba: u64, port_num: u8) {
    if port_num as usize >= AHCI_PORT_MAX {
        kerror!("ahci: port {} is out of range\n", port_num);
        return;
    }

    if unsafe { !AHCI_DMA_READY } {
        dma_window_init();
    }

    let hba_mem = hba as *mut AhciHbaMem;
    let window = AHCI_WINDOW_BASE + port_num as u64 * AHCI_WINDOW_SIZE;

    unsafe {
        (*hba_mem).ghc |= AHCI_GHC_AE;
        let port = &mut (*hba_mem).ports[port_num as usize];

        port.cmd &= !AHCI_PCMD_FRE;
        port.cmd &= !AHCI_PCMD_ST;

        for _ in 0..AHCI_CR_SPINS {
            if port.cmd & AHCI_PCMD_CR == 0 {
                break;
            }
            core::arch::asm!("pause", options(nomem, nostack));
        }
        if port.cmd & AHCI_PCMD_CR != 0 {
            kpanic!(
                "ahci: port {} never went idle, PxCMD=0x{:08X}\n",
                port_num,
                port.cmd
            );
        }
        for _ in 0..AHCI_FR_SPINS {
            if port.cmd & AHCI_PCMD_FR == 0 {
                break;
            }
            core::arch::asm!("pause", options(nomem, nostack));
        }
        if port.cmd & AHCI_PCMD_FR != 0 {
            kpanic!("FR Engine Failed To Stop <-ahci_init()");
        }

        port.is = 0xFFFF_FFFF;
        port.serr = 0xFFFF_FFFF;

        let cl_phys = map_page(window + AHCI_CL_OFF);
        let fb_phys = map_page(window + AHCI_FB_OFF);
        let ct_phys = map_page(window + AHCI_CT_OFF);

        if cl_phys == mm::PMM_NO_PAGE
            || fb_phys == mm::PMM_NO_PAGE
            || ct_phys == mm::PMM_NO_PAGE
        {
            kerror!("ahci: port {} is out of memory\n", port_num);
            return;
        }

        reset_page(window + AHCI_CL_OFF);
        reset_page(window + AHCI_FB_OFF);
        reset_page(window + AHCI_CT_OFF);

        let header = (window + AHCI_CL_OFF) as *mut AhciCmdHeader;
        (*header).flags = AHCI_CL_CFL_5 | AHCI_CL_CBR;
        (*header).prdtl = 0;
        (*header).prdbc = 0;
        (*header).ctba = (ct_phys & 0xFFFF_FFFF) as u32;
        (*header).ctbau = (ct_phys >> 32) as u32;

        port.clb = cl_phys;
        port.fb = fb_phys;

        core::arch::asm!("mfence", options(nostack));

        port.cmd |= AHCI_PCMD_FRE;
        port.cmd |= AHCI_PCMD_ST;
        kinfo!(
            "ahci: port {} up, PxCLB=0x{:X} PxFB=0x{:X} CT=0x{:X}\n",
            port_num,
            cl_phys,
            fb_phys,
            ct_phys
        );
    }

    if let Some((sectors, lba48)) = identify(hba, port_num) {
        unsafe {
            AHCI_DEVICES[AHCI_DEVICE_COUNT] = AhciDevice {
                present: true,
                hba,
                port: port_num,
                sectors,
                lba48,
            };
            AHCI_DEVICE_COUNT += 1;
            kinfo!(
                "ahci: registered disk {} on port {}\n",
                AHCI_DEVICE_COUNT - 1,
                port_num
            );
        }
    }
}

pub fn disk_count() -> usize {
    unsafe { AHCI_DEVICE_COUNT }
}

pub fn disk_sectors(disk: usize) -> u64 {
    unsafe {
        if disk >= AHCI_DEVICE_COUNT {
            return 0;
        }
        AHCI_DEVICES[disk].sectors
    }
}

fn device(disk: usize) -> Option<&'static AhciDevice> {
    unsafe {
        if disk >= AHCI_DEVICE_COUNT || !AHCI_DEVICES[disk].present {
            None
        } else {
            Some(&AHCI_DEVICES[disk])
        }
    }
}

pub fn read(disk: usize, lba: u64, buffer: &mut [u8]) -> bool {
    let Some(dev) = device(disk) else {
        kerror!("ahci: read: unknown disk {}\n", disk);
        return false;
    };

    if lba >= (1u64 << 48) || lba + (buffer.len() as u64 / AHCI_SECTOR_SIZE as u64) > dev.sectors {
        kerror!("ahci: read: LBA out of range\n");
        return false;
    }

    unsafe { xfer(dev, AHCI_CMD_READ_DMA_EXT, lba, buffer.as_ptr(), buffer.len(), false) }
}

pub fn write(disk: usize, lba: u64, buffer: &[u8]) -> bool {
    let Some(dev) = device(disk) else {
        kerror!("ahci: write: unknown disk {}\n", disk);
        return false;
    };

    if lba >= (1u64 << 48) || lba + (buffer.len() as u64 / AHCI_SECTOR_SIZE as u64) > dev.sectors {
        kerror!("ahci: write: LBA out of range\n");
        return false;
    }

    unsafe { xfer(dev, AHCI_CMD_WRITE_DMA_EXT, lba, buffer.as_ptr(), buffer.len(), true) }
}

pub fn flush(disk: usize) -> bool {
    let Some(dev) = device(disk) else {
        kerror!("ahci: flush: unknown disk {}\n", disk);
        return false;
    };

    unsafe { xfer(dev, AHCI_CMD_FLUSH_CACHE_EXT, 0, dma_virt() as *const u8, AHCI_SECTOR_SIZE, false) }
}