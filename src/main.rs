//! Pinx: a small x86-64 kernel, in Rust from the entry point down.
//!
//! Limine loads the ELF, sets up long mode, the HHDM and a framebuffer, then
//! jumps to `kmain`. From there this module brings up the console, the
//! descriptor tables, memory, the APIC and the keyboard, then hands control to
//! the idle loop.

#![no_std]
#![no_main]
#![allow(static_mut_refs)]
// The scheduler and shell are ported in full but deliberately left unstarted,
// matching the original `kmain`, which has `init_shell()` and
// `scheduler_init()` commented out until there is a filesystem. That makes a
// large part of the kernel API unreachable from `main`, and the dead-code lint
// would otherwise drown the build in warnings.
#![allow(dead_code)]

#[cfg(feature = "ahci")]
mod ahci;
mod apic;
mod asm;
mod block;
mod crc;
mod delay;
mod exceptions;
mod ext4;
mod font;
#[cfg(feature = "ahci")]
mod fs;
mod gdt;
#[cfg(feature = "ahci")]
mod gpt;
mod graphics;
mod idt;
mod interrupts;
mod io;
mod keyboard;
mod klog;
mod limine;
mod mm;
mod pci;
mod scheduler;
mod shell;
mod smp;
mod spinlock;
mod syscall;
mod terminal;
#[cfg(feature = "ahci")]
mod user;
#[path = "../pinx_abi.rs"]
pub mod user_abi;

use crate::limine::{framebuffer_response, tsc_frequency_response};
use crate::terminal::halt;

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    unsafe {
        let response = framebuffer_response();
        if response.is_null() || (*response).framebuffer_count < 1 {
            halt();
        }

        let framebuffer = *(*response).framebuffers;
        graphics::init(framebuffer);
    }

    klog::init();
    kinfo!("GRAPHICS_SUB: OK\n");

    gdt::init_cpu(0);
    kinfo!("GDT: OK\n");

    idt::init();
    kinfo!("IDT: OK\n");

    mm::pmm_init();
    kinfo!("PMM INIT: OK\n");

    mm::vmm_init();
    kinfo!("VMM INIT: OK\n");

    mm::kheap_init();
    kinfo!("KERNEL HEAP INIT: OK\n");

    let tsc_freq = {
        let response = tsc_frequency_response();
        if response.is_null() {
            kinfo!("Using tsc_freq from kernel. Not accurate\n");
            2_000_000_000u32
        } else {
            kinfo!("Using tsc_freq from limine.\n");
            unsafe { (*response).frequency as u32 }
        }
    };

    apic::init_bsp();
    apic::set_tsc_freq(tsc_freq);
    kinfo!("x2APIC: OK\n");

    keyboard::init();
    kinfo!("KEYBOARD: OK\n");

    delay::init(tsc_freq);
    kinfo!("APIC TIMER: OK\n");

    io::irq_enable();
    kinfo!("IRQ: OK\n");
    #[cfg(feature = "ahci")] // No other system use's pci system except than ahci.
    pci::enumerate();

    // Discover the other processors, build one idle task per CPU, then release
    // the APs. They must not be freed before `init_global` has sized the
    // per-CPU tables.
    smp::discover();
    kinfo!("SMP: {} CPU(s)\n", smp::cpu_count());
    scheduler::init_global();
    scheduler::init_cpu(0);
    smp::start_aps();

    #[cfg(feature = "ahci")]
    {
        fs::init();
        // Loads /init and registers it as a ring-3 task. If there is nothing
        // valid to run, fall back to the kernel shell on this CPU.
        if !user::try_start() {
            kinfo!("user: no /init, running kernel shell\n");
            shell::init_shell();
            shell::run_shell();
        }
    }

    // Nothing left to do on the BSP: idle until the scheduler hands us a task.
    scheduler::idle_loop()
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    kprint!("\nKERNEL PANIC: {}\n", info);
    halt()
}
