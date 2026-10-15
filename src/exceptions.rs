//! CPU exception handlers.
//!
//! Each handler reports what happened and then stops the CPU. There is no
//! recovery path: the original kernel never returns from these either, which
//! is why the assembly stubs have no `iretq`.

use crate::kprint;
use crate::limine;
use crate::terminal::halt;

fn hex(label: &str, value: u64) {
    kprint!("{}{:#018x}\n", label, value);
}

fn fatal(name: &str) -> ! {
    kprint!("EXCEPTION: {}\n", name);
    halt();
}

#[no_mangle]
pub extern "C" fn exception_divide_error() -> ! {
    fatal("Divide Error (0)")
}

#[no_mangle]
pub extern "C" fn exception_debug() -> ! {
    fatal("Debug (1)")
}

#[no_mangle]
pub extern "C" fn exception_nmi() -> ! {
    fatal("Non-Maskable Interrupt (2)")
}

#[no_mangle]
pub extern "C" fn exception_breakpoint() -> ! {
    fatal("Breakpoint (3)")
}

#[no_mangle]
pub extern "C" fn exception_overflow() -> ! {
    fatal("Overflow (4)")
}

#[no_mangle]
pub extern "C" fn exception_bound_range() -> ! {
    fatal("Bound Range (5)")
}

#[no_mangle]
pub extern "C" fn exception_invalid_opcode() -> ! {
    fatal("Invalid Opcode (6)")
}

#[no_mangle]
pub extern "C" fn exception_device_not_available() -> ! {
    fatal("Device Not Available (7)")
}

#[no_mangle]
pub extern "C" fn exception_double_fault() -> ! {
    fatal("Double Fault (8)")
}

#[no_mangle]
pub extern "C" fn exception_invalid_tss() -> ! {
    fatal("Invalid TSS (10)")
}

#[no_mangle]
pub extern "C" fn exception_segment_not_present() -> ! {
    fatal("Segment Not Present (11)")
}

#[no_mangle]
pub extern "C" fn exception_stack_segment_fault() -> ! {
    fatal("Stack-Segment Fault (12)")
}

#[no_mangle]
pub extern "C" fn exception_general_protection() -> ! {
    fatal("General Protection Fault (13)")
}

#[no_mangle]
pub extern "C" fn exception_page_fault(rip: u64, error_code: u64) -> ! {
    let cr2: u64;
    unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack)) };

    kprint!("EXCEPTION: Page Fault (14)\n");
    hex("CR2: ", cr2);
    hex("RIP: ", rip);
    kprint!("ERR: ");
    hex("", error_code);
    kprint!("(bit0=P present, bit1=W write, bit2=U user)\n");

    let hhdm = {
        let response = limine::hhdm_response();
        if response.is_null() {
            0
        } else {
            unsafe { (*response).offset }
        }
    };
    hex("HHDM: ", hhdm);

    if cr2 == 0xFFFF_9000_0000_0000 {
        kprint!("NOTE: CR2 == KHEAP_START (heap base was not mapped)\n");
    }

    halt()
}

#[no_mangle]
pub extern "C" fn exception_alignment_check() -> ! {
    fatal("Alignment Check (17)")
}

#[no_mangle]
pub extern "C" fn exception_machine_check() -> ! {
    fatal("Machine Check (18)")
}

#[no_mangle]
pub extern "C" fn exception_simd_floating_point() -> ! {
    fatal("SIMD Floating-Point (19)")
}

#[no_mangle]
pub extern "C" fn exception_control_protection() -> ! {
    fatal("Control Protection (21)")
}
