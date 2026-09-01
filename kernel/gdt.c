#include <stdint.h>
#include "includes/pinx/gdt.h"

struct gdt_entry {
    uint16_t limit_low;
    uint16_t base_low;

    uint8_t base_middle;

    uint8_t access;
    uint8_t granularity;

    uint8_t base_high;
} __attribute__((packed));

struct gdt_ptr {
    uint16_t limit;
    uint64_t base;
} __attribute__((packed));

extern void gdt_flush(const struct gdt_ptr *ptr);

static struct gdt_entry gdt[3];
static struct gdt_ptr gdtr;

static void gdt_set_entry(
    int index,
    uint32_t base,
    uint32_t limit,
    uint8_t access,
    uint8_t granularity
)
{
    gdt[index].base_low = (uint16_t)(base & 0xFFFF);
    gdt[index].base_middle = (uint8_t)((base >> 16) & 0xFF);
    gdt[index].base_high = (uint8_t)((base >> 24) & 0xFF);

    gdt[index].limit_low = (uint16_t)(limit & 0xFFFF);

    gdt[index].granularity =
        (uint8_t)(((limit >> 16) & 0x0F) |
        (granularity & 0xF0));

    gdt[index].access = access;
}

void gdt_init(void)
{
    gdtr.limit = (uint16_t)(sizeof(gdt) - 1);
    gdtr.base = (uint64_t)&gdt;

    /*
     * Entry 0:
     * Null descriptor
     */
    gdt_set_entry(
        0,
        0,
        0,
        0x00,
        0x00
    );

    /*
     * Entry 1:
     * Kernel 64-bit code
     *
     * Access:
     *   Present
     *   Ring 0
     *   Code segment
     *   Readable
     *
     * Granularity:
     *   Long mode
     */
    gdt_set_entry(
        1,
        0,
        0,
        0x9A,
        0xA0
    );

    /*
     * Entry 2:
     * Kernel data
     *
     * Access:
     *   Present
     *   Ring 0
     *   Data segment
     *   Writable
     */
    gdt_set_entry(
        2,
        0,
        0,
        0x92,
        0x00
    );

    /*
     * Load the new GDT and reload segment registers.
     */
    gdt_flush(&gdtr);
}