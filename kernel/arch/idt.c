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
extern void isr1(void);
extern void isr2(void);
extern void isr3(void);
extern void isr4(void);
extern void isr5(void);
extern void isr6(void);
extern void isr7(void);
extern void isr8(void);
extern void isr10(void);
extern void isr11(void);
extern void isr12(void);
extern void isr13(void);
extern void isr14(void);
extern void isr17(void);
extern void isr18(void);
extern void isr19(void);
extern void isr21(void);

extern void irq32(void);
extern void irq33(void);
extern void irq34(void);
extern void irq35(void);
extern void irq36(void);
extern void irq37(void);
extern void irq38(void);
extern void irq39(void);
extern void irq40(void);
extern void irq41(void);
extern void irq42(void);
extern void irq43(void);
extern void irq44(void);
extern void irq45(void);
extern void irq46(void);
extern void irq47(void);

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
    idt_set_gate(1, (uint64_t)isr1, 0x08, 0x8E);
    idt_set_gate(2, (uint64_t)isr2, 0x08, 0x8E);
    idt_set_gate(3, (uint64_t)isr3, 0x08, 0x8E);
    idt_set_gate(4, (uint64_t)isr4, 0x08, 0x8E);
    idt_set_gate(5, (uint64_t)isr5, 0x08, 0x8E);
    idt_set_gate(6, (uint64_t)isr6, 0x08, 0x8E);
    idt_set_gate(7, (uint64_t)isr7, 0x08, 0x8E);
    idt_set_gate(8, (uint64_t)isr8, 0x08, 0x8E);
    idt_set_gate(10, (uint64_t)isr10, 0x08, 0x8E);
    idt_set_gate(11, (uint64_t)isr11, 0x08, 0x8E);
    idt_set_gate(12, (uint64_t)isr12, 0x08, 0x8E);
    idt_set_gate(13, (uint64_t)isr13, 0x08, 0x8E);
    idt_set_gate(14, (uint64_t)isr14, 0x08, 0x8E);
    idt_set_gate(17, (uint64_t)isr17, 0x08, 0x8E);
    idt_set_gate(18, (uint64_t)isr18, 0x08, 0x8E);
    idt_set_gate(19, (uint64_t)isr19, 0x08, 0x8E);
    idt_set_gate(21, (uint64_t)isr21, 0x08, 0x8E);

    idt_set_gate(32, (uint64_t)irq32, 0x08, 0x8E);
    idt_set_gate(33, (uint64_t)irq33, 0x08, 0x8E);
    idt_set_gate(34, (uint64_t)irq34, 0x08, 0x8E);
    idt_set_gate(35, (uint64_t)irq35, 0x08, 0x8E);
    idt_set_gate(36, (uint64_t)irq36, 0x08, 0x8E);
    idt_set_gate(37, (uint64_t)irq37, 0x08, 0x8E);
    idt_set_gate(38, (uint64_t)irq38, 0x08, 0x8E);
    idt_set_gate(39, (uint64_t)irq39, 0x08, 0x8E);
    idt_set_gate(40, (uint64_t)irq40, 0x08, 0x8E);
    idt_set_gate(41, (uint64_t)irq41, 0x08, 0x8E);
    idt_set_gate(42, (uint64_t)irq42, 0x08, 0x8E);
    idt_set_gate(43, (uint64_t)irq43, 0x08, 0x8E);
    idt_set_gate(44, (uint64_t)irq44, 0x08, 0x8E);
    idt_set_gate(45, (uint64_t)irq45, 0x08, 0x8E);
    idt_set_gate(46, (uint64_t)irq46, 0x08, 0x8E);
    idt_set_gate(47, (uint64_t)irq47, 0x08, 0x8E);

    __asm__ volatile ("lidt %0" : : "m"(idtr));
}
