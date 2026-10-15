//! /init: the first PINX userspace program.
//!
//! The kernel loads this ELF, checks for the [`PinxInitHeader`] below, then
//! jumps to `_start` on a fresh user stack. There is no `argc`/`argv`/`envp`.
//! Write your program in [`main`]; everything around it is boilerplate that
//! must stay for the kernel to accept and enter the binary.

#![no_std]
#![no_main]

use core::panic::PanicInfo;

use pinx_libc::{exit, println, sched_yield, spawn};

// Single source of truth, shared with the kernel. If this program moves, point
// the path at pinx_kernel/pinx_abi.rs (or copy that file next to this one).
#[path = "../../pinx_abi.rs"]
mod pinx_abi;

use pinx_abi::PinxInitHeader;

/// Kernel entry point, named by `linker.ld` (`ENTRY(_start)`). Do not rename.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Make sure the header object survives linking even when rustc splits the
    // crate into several codegen units; the loader rejects us without it.
    core::hint::black_box(&PINX_INIT_HEADER);
    main()
}

/// Your program starts here.
fn main() -> ! {
    println!("\n/INIT: Loaded");

    // Start every program we know about, then go do init things. `spawn` does
    // not wait: it returns as soon as the task is registered, so init is free
    // to launch the next one straight away.
    for app in ["hello"] {
        let pid = spawn(app);
        if pid == u64::MAX {
            println!("/INIT: could not start {}", app);
        } else {
            println!("/INIT: started {} as pid {}", app, pid);
        }
    }

    // Nothing left to launch: sit in the init loop, yielding so the other tasks
    // get the CPU.
    loop {
        sched_yield();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("init: panic: {}", info);
    exit(0)
}

/// Marks this ELF as a PINX init so the kernel will run it.
#[used]
#[unsafe(no_mangle)]
pub static PINX_INIT_HEADER: PinxInitHeader = PinxInitHeader::new();
