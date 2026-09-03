.code64

.section .text

/*
 * All exception stubs are intentionally minimal on purpose:
 * they disable interrupts, call their C handler, and the
 * handler never returns (it reports the fault and halts the CPU
 * in an infinite loop). There is no IRETQ because we never come
 * back from a fatal exception.
 *
 * For CPU-generated exceptions that include an error code
 * (8, 10, 11, 12, 13, 14, 17, 21) the CPU pushes the error code
 * onto the stack before dispatching. Because the C handler never
 * returns, we do not need to clean it up.
 */

.global isr0
.type   isr0, @function
isr0:
    cli
    call exception_divide_error
1:
    hlt
    jmp 1b
.size isr0, .-isr0

.global isr1
.type   isr1, @function
isr1:
    cli
    call exception_debug
1:
    hlt
    jmp 1b
.size isr1, .-isr1

.global isr2
.type   isr2, @function
isr2:
    cli
    call exception_nmi
1:
    hlt
    jmp 1b
.size isr2, .-isr2

.global isr3
.type   isr3, @function
isr3:
    cli
    call exception_breakpoint
1:
    hlt
    jmp 1b
.size isr3, .-isr3

.global isr4
.type   isr4, @function
isr4:
    cli
    call exception_overflow
1:
    hlt
    jmp 1b
.size isr4, .-isr4

.global isr5
.type   isr5, @function
isr5:
    cli
    call exception_bound_range
1:
    hlt
    jmp 1b
.size isr5, .-isr5

.global isr6
.type   isr6, @function
isr6:
    cli
    call exception_invalid_opcode
1:
    hlt
    jmp 1b
.size isr6, .-isr6

.global isr7
.type   isr7, @function
isr7:
    cli
    call exception_device_not_available
1:
    hlt
    jmp 1b
.size isr7, .-isr7

.global isr8
.type   isr8, @function
isr8:
    cli
    call exception_double_fault
1:
    hlt
    jmp 1b
.size isr8, .-isr8

.global isr10
.type   isr10, @function
isr10:
    cli
    call exception_invalid_tss
1:
    hlt
    jmp 1b
.size isr10, .-isr10

.global isr11
.type   isr11, @function
isr11:
    cli
    call exception_segment_not_present
1:
    hlt
    jmp 1b
.size isr11, .-isr11

.global isr12
.type   isr12, @function
isr12:
    cli
    call exception_stack_segment_fault
1:
    hlt
    jmp 1b
.size isr12, .-isr12

.global isr13
.type   isr13, @function
isr13:
    cli
    call exception_general_protection
1:
    hlt
    jmp 1b
.size isr13, .-isr13

.global isr14
.type   isr14, @function
isr14:
    cli
    call exception_page_fault
1:
    hlt
    jmp 1b
.size isr14, .-isr14

.global isr17
.type   isr17, @function
isr17:
    cli
    call exception_alignment_check
1:
    hlt
    jmp 1b
.size isr17, .-isr17

.global isr18
.type   isr18, @function
isr18:
    cli
    call exception_machine_check
1:
    hlt
    jmp 1b
.size isr18, .-isr18

.global isr19
.type   isr19, @function
isr19:
    cli
    call exception_simd_floating_point
1:
    hlt
    jmp 1b
.size isr19, .-isr19

.global isr21
.type   isr21, @function
isr21:
    cli
    call exception_control_protection
1:
    hlt
    jmp 1b
.size isr21, .-isr21
