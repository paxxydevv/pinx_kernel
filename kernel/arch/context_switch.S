.code64

.section .text

/*
 * Cooperative context switch.
 *
 * SysV AMD64 calling convention:
 *   RDI = pointer to where the current RSP should be saved (uint64_t *)
 *   RSI = RSP of the task to switch to (uint64_t)
 *
 * Only callee-saved registers (RBX, RBP, R12-R15) survive across a
 * function call, so those are the only ones the CPU does not already
 * save for us. They are pushed onto the current task's stack, the
 * stack pointer is saved, and the target task's stack pointer is
 * loaded. The target's callee-saved registers are then popped and
 * `ret` resumes it at the instruction after its own context_switch
 * call (or at its entry point for a freshly created task).
 */

.global context_switch
.type   context_switch, @function
context_switch:
    pushq %r15
    pushq %r14
    pushq %r13
    pushq %r12
    pushq %rbp
    pushq %rbx

    movq %rsp, (%rdi)
    movq %rsi, %rsp

    popq %rbx
    popq %rbp
    popq %r12
    popq %r13
    popq %r14
    popq %r15
    ret
.size   context_switch, .-context_switch