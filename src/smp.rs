//! Symmetric multiprocessing via the Limine SMP protocol.
//!
//! The bootloader has already reset and parked the application processors; we
//! only have to claim each one by writing its `goto_address` and let the
//! bootloader release it there. Each CPU then brings up its own GDT/TSS (so it
//! has a private `rsp0`), shares the kernel IDT and page tables, enables its
//! own local APIC and timer, and drops into the scheduler's idle loop.

use crate::limine::{self, LimineSmpInfo};
use crate::{apic, gdt, idt, io, scheduler};

static mut CPU_COUNT: usize = 1;
static mut LAPIC_IDS: [u32; gdt::MAX_CPUS] = [0; gdt::MAX_CPUS];

/// How many CPUs the kernel is actually using (boot processor included).
pub fn cpu_count() -> usize {
    unsafe { CPU_COUNT }
}

/// Map this CPU's x2APIC id back to a dense index in `[0, cpu_count())`.
///
/// The per-CPU tables are indexed, but the hardware only hands out x2APIC ids,
/// which need not be contiguous. Slot 0 is always the bootstrap processor.
pub fn cpu_index() -> usize {
    let id = apic::local_id();
    unsafe {
        let mut i = 0;
        while i < CPU_COUNT {
            if LAPIC_IDS[i] == id {
                return i;
            }
            i += 1;
        }
    }
    // Unknown CPU: fall back to the BSP rather than corrupting another slot.
    0
}

/// Read the bootloader's processor list and reserve an index per CPU, but do
/// not start anything yet. `scheduler::init_global` needs to know the count
/// before it can build one idle task per CPU, and the APs must not be released
/// until that is done, hence the two phases.
pub fn discover() {
    let response = limine::smp_response();
    if response.is_null() {
        unsafe {
            CPU_COUNT = 1;
            LAPIC_IDS[0] = apic::local_id();
        }
        return;
    }

    unsafe {
        let response = &*response;
        let bsp = response.bsp_lapic_id;

        LAPIC_IDS[0] = bsp;
        let mut index = 1usize;

        let mut i = 0u64;
        while i < response.cpu_count {
            let info = *response.cpus.add(i as usize);
            if !info.is_null() && (*info).lapic_id != bsp && index < gdt::MAX_CPUS {
                LAPIC_IDS[index] = (*info).lapic_id;
                index += 1;
            }
            i += 1;
        }

        CPU_COUNT = index;
    }
}

/// Release the application processors into [`ap_entry`].
pub fn start_aps() {
    let response = limine::smp_response();
    if response.is_null() {
        return;
    }

    unsafe {
        let response = &*response;
        let bsp = response.bsp_lapic_id;

        let mut index = 1usize;
        let mut i = 0u64;
        while i < response.cpu_count {
            let info = *response.cpus.add(i as usize);
            if !info.is_null() && (*info).lapic_id != bsp && index < gdt::MAX_CPUS {
                // `extra_argument` must be visible before the AP runs, and the
                // AP runs the instant `goto_address` is written, so set it
                // first.
                (*info).extra_argument = index as u64;
                (*info).goto_address = Some(ap_entry);
                index += 1;
            }
            i += 1;
        }
    }
}

/// Application processor entry point, called by the bootloader with the
/// processor's own `LimineSmpInfo`. Never returns; the bootloader's `goto`
/// contract only has a `void` return, so the final idle loop is where this ends
/// up living.
extern "C" fn ap_entry(info: *mut LimineSmpInfo) {
    let cpu = unsafe { (*info).extra_argument as usize };

    gdt::init_cpu(cpu);
    idt::load();
    apic::init_ap();
    apic::timer_init_cpu();

    scheduler::init_cpu(cpu);
    io::irq_enable();

    scheduler::idle_loop()
}
