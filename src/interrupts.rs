//! Shared IRQ entry point.
//!
//! The assembly stubs in `asm.rs` push a uniform frame and call
//! `irq_common_handler`. The field order below matches the push order exactly,
//! since the handler reads the vector straight out of the saved stack.

use crate::{apic, keyboard, scheduler};

#[repr(C)]
pub struct InterruptFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub vector: u64,
    pub error_code: u64,
}

/// Shared IRQ entry point. Returns the frame the CPU should resume; normally
/// that is `frame` itself, but the timer may hand back a different task's frame
/// after a context switch.
#[no_mangle]
pub extern "C" fn irq_common_handler(frame: *mut InterruptFrame) -> *mut InterruptFrame {
    let vector = unsafe { (*frame).vector };

    // Software syscalls share this entry path but have no device behind them,
    // so they return without an EOI.
    if vector == crate::idt::SYSCALL_VECTOR as u64 {
        return crate::syscall::dispatch(frame);
    }

    if vector == apic::TIMER_VECTOR as u64 {
        apic::timer_irq();
        apic::send_eoi();
        // Wake anything whose sleep has expired before choosing the next task.
        scheduler::wake_sleepers(apic::uptime_ms());
        // Preempt: this is where the round-robin switch happens.
        return unsafe { scheduler::schedule(frame) };
    }

    if vector == keyboard::IRQ_VECTOR as u64 {
        keyboard::irq();
        // A blocked reader can now make progress.
        scheduler::wake_input_waiters();
    }

    apic::send_eoi();
    frame
}

/// Nothing to acknowledge and nothing to service, but the EOI is still sent:
/// vector 15 comes from the 8259, which does need one, and a superfluous EOI
/// on the APIC is a no-op for an interrupt that never reached the in-service
/// register.
#[no_mangle]
pub extern "C" fn irq_spurious_handler(frame: *mut InterruptFrame) -> *mut InterruptFrame {
    apic::send_eoi();
    frame
}
