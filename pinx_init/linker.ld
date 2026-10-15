/* Link the program into the lower canonical half.
 *
 * The kernel maps each PT_LOAD segment at its p_vaddr, so the base is ours to
 * choose; 0x400000 keeps clear of the null page. `_start` is supplied by
 * src/main.rs. */
ENTRY(_start)

SECTIONS
{
    . = 0x400000;

    .text : { *(.text .text.*) }
    .rodata : { *(.rodata .rodata.*) }
    .data : { *(.data .data.*) }
    .bss : { *(.bss .bss.*) *(COMMON) }
}
