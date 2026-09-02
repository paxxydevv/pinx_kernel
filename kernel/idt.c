#include "pinx/idt.h"
#include <stdint.h>
struct idt_entry {
    uint16_t offset_low;
    uint16_t selector;
    uint8_t  ist;
    uint8_t  type_attr;
    uint16_t offset_mid;
    uint32_t offset_high;
    uint32_t reserved;
} __attribute__((packed));

struct idt_ptr {
    uint16_t limit;
    uint64_t base;
} __attribute__((packed));

static struct idt_entry idt[256];
static struct idt_ptr idtr;
extern void isr0(void);
static void idt_set_gate(
    uint8_t vector,
    uint64_t handler,
    uint16_t selector,
    uint8_t type_attr
)
{
    idt[vector].offset_low  = (uint16_t)(handler & 0xFFFF);
    idt[vector].selector    = selector;
    idt[vector].ist         = 0;
    idt[vector].type_attr   = type_attr;
    idt[vector].offset_mid  = (uint16_t)((handler >> 16) & 0xFFFF);
    idt[vector].offset_high = (uint32_t)((handler >> 32) & 0xFFFFFFFF);
    idt[vector].reserved    = 0;
}
void idt_init(void) {
    for (uint16_t i = 0; i < 256; i++) {
        idt[i] = (struct idt_entry){0};
    }
    idtr.limit = (uint16_t)(sizeof(idt) - 1);
    idtr.base  = (uint64_t)&idt;
    idt_set_gate(0, (uint64_t)isr0, 0x08, 0x8E);
    __asm__ volatile ("lidt %0" : : "m"(idtr));
}
