.code64

.section .text

.macro irq_stub name, vector
.global \name
.type   \name, @function
\name:
    cli
    pushq $0
    pushq $\vector
    jmp irq_common
.size \name, .-\name
.endm

irq_common:
    pushq %rax
    pushq %rbx
    pushq %rcx
    pushq %rdx
    pushq %rsi
    pushq %rdi
    pushq %rbp
    pushq %r8
    pushq %r9
    pushq %r10
    pushq %r11
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15

    movq %rsp, %rdi
    call irq_common_handler

    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %r11
    popq %r10
    popq %r9
    popq %r8
    popq %rbp
    popq %rdi
    popq %rsi
    popq %rdx
    popq %rcx
    popq %rbx
    popq %rax

    addq $16, %rsp
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
