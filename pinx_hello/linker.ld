/* Link the program into the lower canonical half.
 *
 * The kernel maps each PT_LOAD segment at its p_vaddr. Every program in the
 * shared address space picks a distinct base: init owns 0x400000, this one
 * starts at 0x800000. `_start` is supplied by src/main.rs. */
ENTRY(_start)

SECTIONS
{
    . = 0x800000;

    .text : { *(.text .text.*) }
    .rodata : { *(.rodata .rodata.*) }
    .data : { *(.data .data.*) }
    .bss : { *(.bss .bss.*) *(COMMON) }
}
