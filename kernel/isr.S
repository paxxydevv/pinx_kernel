.code64

.section .text

.global isr0
.type isr0, @function

.extern exception_divide_error

isr0:
    cli

    call exception_divide_error

1:
    hlt
    jmp 1b

.size isr0, .-isr0