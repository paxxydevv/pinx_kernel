//! Low-level entry points that cannot be expressed as ordinary Rust.
//!
//! These are the same stubs the kernel used to keep in `.S` files, written
//! here with `global_asm!` so there is no assembly source to build separately.
//! Everything uses Intel syntax, which is `global_asm!`'s default.

use crate::gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};
use core::arch::{asm, global_asm};

// Cooperative context switch.
//
//   RDI = pointer to where the current RSP should be saved (uint64_t *)
//   RSI = RSP of the task to switch to (uint64_t)
//
// Only callee-saved registers (RBX, RBP, R12-R15) survive a call, so those are
// pushed, the stack pointer is swapped, and the target's registers are popped.
// `ret` then resumes at the instruction after the target's own context_switch
// call, or at its entry point for a freshly created task.
global_asm!(
    r#"
.section .text
.global context_switch
context_switch:
    push r15
    push r14
    push r13
    push r12
    push rbp
    push rbx

    mov [rdi], rsp
    mov rsp, rsi

    pop rbx
    pop rbp
    pop r12
    pop r13
    pop r14
    pop r15
    ret
"#
);

// Load the GDT and reload every segment register.
//
// 0x10 = GDT entry 2 (kernel data), 0x08 = GDT entry 1 (kernel code). CS cannot
// be `mov`ed into, so it is reloaded with a far return.
global_asm!(
    r#"
.section .text
.global gdt_flush
gdt_flush:
    lgdt [rdi]

    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    push 0x08
    lea rax, [rip + 1f]
    push rax
    retfq
1:
    ret
"#
);

// Hardware interrupt stubs. Each pushes a dummy error code (so the frame has
// the same shape everywhere) and its vector, then falls into the common path
// that saves the volatile registers, calls the Rust handler, restores them and
// returns with `iretq`.
//
// 15 and 255 are the vectors the hardware can raise on its own with no device
// behind them: the 8259's spurious IRQ7 and the local APIC's spurious interrupt.
global_asm!(
    r#"
.section .text

.macro irq_stub name, vector
.global \name
\name:
    cli
    push 0
    push \vector
    jmp irq_common
.endm

irq_common:
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    mov rdi, rsp
    call irq_common_handler

    # The handler returns the frame to resume in rax; it may differ from the one
    # we passed in when the scheduler switched tasks. Adopt it before restoring.
    mov rsp, rax

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax

    add rsp, 16
    iretq

irq_stub irq32, 32
irq_stub irq33, 33
irq_stub irq34, 34
irq_stub irq35, 35
irq_stub irq36, 36
irq_stub irq37, 37
irq_stub irq38, 38
irq_stub irq39, 39
irq_stub irq40, 40
irq_stub irq41, 41
irq_stub irq42, 42
irq_stub irq43, 43
irq_stub irq44, 44
irq_stub irq45, 45
irq_stub irq46, 46
irq_stub irq47, 47

irq_stub spurious_irq15, 15
irq_stub spurious_irq255, 255

// Software syscall entry. `int 0x80` is invoked from ring 3, so the CPU has
// already switched to the TSS `rsp0` stack and pushed SS/RSP/RFLAGS/CS/RIP with
// no error code. The stub pushes a dummy one so the frame matches InterruptFrame
// exactly, then shares the common save/call/restore path with the IRQs.
irq_stub int80, 0x80
"#
);

// CPU exception stubs. They disable interrupts, call their handler and, since
// every handler halts, never return. Exceptions that push an error code (8, 10,
// 11, 12, 13, 14, 17, 21) leave it on the stack; the handler never returns, so
// it does not have to be cleaned up.
global_asm!(
    r#"
.section .text

.macro isr_stub name, handler
.global \name
\name:
    cli
    call \handler
1:
    hlt
    jmp 1b
.endm

isr_stub isr0, exception_divide_error
isr_stub isr1, exception_debug
isr_stub isr2, exception_nmi
isr_stub isr3, exception_breakpoint
isr_stub isr4, exception_overflow
isr_stub isr5, exception_bound_range
isr_stub isr6, exception_invalid_opcode
isr_stub isr7, exception_device_not_available
isr_stub isr8, exception_double_fault
isr_stub isr10, exception_invalid_tss
isr_stub isr11, exception_segment_not_present
isr_stub isr12, exception_stack_segment_fault
isr_stub isr13, exception_general_protection
isr_stub isr17, exception_alignment_check
isr_stub isr18, exception_machine_check
isr_stub isr19, exception_simd_floating_point
isr_stub isr21, exception_control_protection

.global isr14
isr14:
    cli
    mov rdi, [rsp + 8]
    mov rsi, [rsp]
    call exception_page_fault
1:
    hlt
    jmp 1b
"#
);

/// Leave ring 0 for ring 3: start executing at `entry` on `stack`.
///
/// `iretq` is the only instruction that can lower privilege, and it takes its
/// register values from the stack. A return to a *lower* ring therefore expects
/// a five-word frame -- `SS, RSP, RFLAGS, CS, RIP` -- so this pushes it and
/// jumps into it. No interrupt is involved; the frame is built by hand.
///
/// This never returns. The ring-0 context that called it is abandoned, and the
/// next way back into the kernel is a trap (`int 0x80` or a hardware interrupt),
/// which lands on the TSS `rsp0` stack.
///
/// `entry` and `stack` must be user-accessible (the U/S bit set at every page
/// table level) or the first fetch or push faults. `stack` is the *top* of the
/// user stack and should be 16-byte aligned.
pub unsafe fn enter_user(entry: u64, stack: u64) -> ! {
    // RFLAGS bit 1 is hard-wired to 1; bit 9 (IF) lets interrupts through so
    // the timer and keyboard keep working while user code runs.
    const RFLAGS: u64 = 0x202;
    const USER_CS: u64 = USER_CODE_SELECTOR as u64;
    const USER_SS: u64 = USER_DATA_SELECTOR as u64;

    asm!(
        // iretq pops these five, so push them in the opposite order.
        "push {ss}",
        "push {stack}",
        "push {rflags}",
        "push {cs}",
        "push {entry}",
        // iretq does not reload the data segments, so make them user data while
        // still in ring 0. Entry and stack are already consumed by now, so it
        // is safe if this clobbers the registers holding them.
        "mov ax, {ss}",
        "mov ds, ax",
        "mov es, ax",
        "iretq",
        entry = in(reg) entry,
        stack = in(reg) stack,
        cs = const USER_CS,
        ss = const USER_SS,
        rflags = const RFLAGS,
        options(noreturn),
    )
}
