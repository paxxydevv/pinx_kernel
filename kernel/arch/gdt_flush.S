.code64

.section .text

.global gdt_flush
.type gdt_flush, @function

gdt_flush:
    /*
     * SysV AMD64 calling convention:
     *
     * RDI = pointer to gdt_ptr
     */

    /* Load GDTR */
    lgdt (%rdi)

    /*
     * Reload the data segment registers.
     *
     * 0x10 = GDT entry 2 (kernel data)
     */
    mov $0x10, %ax

    mov %ax, %ds
    mov %ax, %es
    mov %ax, %fs
    mov %ax, %gs
    mov %ax, %ss

    /*
     * Reload CS.
     *
     * 0x08 = GDT entry 1 (kernel code)
     *
     * We cannot simply mov into CS, so perform
     * a far return.
     */
    pushq $0x08

    lea 1f(%rip), %rax
    pushq %rax

    lretq

1:
    ret

.size gdt_flush, .-gdt_flush