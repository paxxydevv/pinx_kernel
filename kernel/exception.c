#include <stdint.h>
#include "pinx/terminal.h"
#include "3party/limine.h"

extern volatile struct limine_hhdm_request hhdm_request;

static void kprintf_hex(const char *label, uint64_t value)
{
    char buf[19];
    buf[0] = '0';
    buf[1] = 'x';
    for (int i = 17; i >= 2; i--) {
        uint64_t nib = value & 0xF;
        buf[i] = (char)(nib < 10 ? '0' + nib : 'a' + (nib - 10));
        value >>= 4;
    }
    buf[18] = '\0';
    kprintf(label);
    kprintf(buf);
    kprintf("\n");
}

static void exception_fatal(const char *name, uint64_t vector)
{
    kprintf("EXCEPTION: ");
    kprintf(name);
    kprintf("\n");
    (void)vector;
    for (;;) {
        __asm__ volatile ("hlt");
    }
}

void exception_divide_error(void)
{
    exception_fatal("Divide Error (0)", 0);
}

void exception_debug(void)
{
    exception_fatal("Debug (1)", 1);
}

void exception_nmi(void)
{
    exception_fatal("Non-Maskable Interrupt (2)", 2);
}

void exception_breakpoint(void)
{
    exception_fatal("Breakpoint (3)", 3);
}

void exception_overflow(void)
{
    exception_fatal("Overflow (4)", 4);
}

void exception_bound_range(void)
{
    exception_fatal("Bound Range (5)", 5);
}

void exception_invalid_opcode(void)
{
    exception_fatal("Invalid Opcode (6)", 6);
}

void exception_device_not_available(void)
{
    exception_fatal("Device Not Available (7)", 7);
}

void exception_double_fault(void)
{
    exception_fatal("Double Fault (8)", 8);
}

void exception_invalid_tss(void)
{
    exception_fatal("Invalid TSS (10)", 10);
}

void exception_segment_not_present(void)
{
    exception_fatal("Segment Not Present (11)", 11);
}

void exception_stack_segment_fault(void)
{
    exception_fatal("Stack-Segment Fault (12)", 12);
}

void exception_general_protection(void)
{
    exception_fatal("General Protection Fault (13)", 13);
}

void exception_page_fault(uint64_t rip, uint64_t error_code)
{
    uint64_t cr2;
    __asm__ volatile ("mov %%cr2, %0" : "=r"(cr2));

    kprintf("EXCEPTION: Page Fault (14)\n");
    kprintf_hex("CR2: ", cr2);
    kprintf_hex("RIP: ", rip);
    kprintf("ERR: ");
    kprintf_hex("", error_code);
    kprintf("(bit0=P present, bit1=W write, bit2=U user)\n");
    kprintf_hex("HHDM: ", hhdm_request.response != 0 ? hhdm_request.response->offset : 0);
    if (cr2 == 0xFFFF900000000000ULL)
        kprintf("NOTE: CR2 == KHEAP_START (heap base was not mapped)\n");
    for (;;) {
        __asm__ volatile ("hlt");
    }
}

void exception_alignment_check(void)
{
    exception_fatal("Alignment Check (17)", 17);
}

void exception_machine_check(void)
{
    exception_fatal("Machine Check (18)", 18);
}

void exception_simd_floating_point(void)
{
    exception_fatal("SIMD Floating-Point (19)", 19);
}

void exception_control_protection(void)
{
    exception_fatal("Control Protection (21)", 21);
}
