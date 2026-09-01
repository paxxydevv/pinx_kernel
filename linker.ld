OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(kmain)

PHDRS
{
    text   PT_LOAD FLAGS(5); /* R + X */
    rodata PT_LOAD FLAGS(4); /* R */
    data   PT_LOAD FLAGS(6); /* R + W */
}

SECTIONS
{
    . = 0xffffffff80000000;

    .text : ALIGN(CONSTANT(MAXPAGESIZE))
    {
        KEEP(*(.text.boot))
        *(.text .text.*)
    } :text

    .rodata : ALIGN(CONSTANT(MAXPAGESIZE))
    {
        *(.rodata .rodata.*)
        *(.limine_requests_start)
        *(.limine_requests)
        *(.limine_requests_end)
        *(.note.gnu.property)
    } :rodata

    .data : ALIGN(CONSTANT(MAXPAGESIZE))
    {
        *(.data .data.*)
    } :data

    .bss : ALIGN(CONSTANT(MAXPAGESIZE))
    {
        *(COMMON)
        *(.bss .bss.*)
    } :data

    /DISCARD/ :
    {
        *(.eh_frame)
        *(.note.GNU-stack)
        *(.comment)
    }
}

