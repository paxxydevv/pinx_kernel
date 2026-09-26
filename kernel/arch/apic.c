#include "pinx/apic.h"
#include "pinx/cpuid.h"
#include "pinx/msr.h"
#include "pinx/io.h"
#include "pinx/terminal.h"
#include "pinx/keyboard.h"
#include "pinx/mm/memory_management.h"
#include <stdint.h>

#define IOAPIC_BASE 0xFEC00000ULL
#define IOAPIC_VIRT 0xFFFFC00000000000ULL
#define IOAPIC_IOREGSEL 0x00
#define IOAPIC_IOWIN 0x10

/* Bit layout of an IOAPIC redirection entry, low half then high half. */
#define IOAPIC_REDIR_DEST_ID_SHIFT 24
#define IOAPIC_REDIR_MASK_SHIFT    16
#define IOAPIC_REDIR_TRIGGER_SHIFT 15
#define IOAPIC_REDIR_POLARITY_SHIFT 13
#define IOAPIC_REDIR_DEST_PHYS_SHIFT 11
#define IOAPIC_REDIR_DEST_MODE_SHIFT 8
#define IOAPIC_REDIR_BASE 0x10

#define PIC1_CMD 0x21
#define PIC2_CMD 0xA1

static volatile uint32_t apic_ticks = 0;

static inline void x2apic_write(uint32_t reg, uint32_t val) {
    wrmsr(reg, val);
}

static volatile uint32_t *ioapic_addr(uint32_t reg) {
    return (volatile uint32_t *)(IOAPIC_VIRT + reg);
}

// static uint32_t ioapic_read(uint32_t reg) {
//    *ioapic_addr(IOAPIC_IOREGSEL) = reg;
//    return *ioapic_addr(IOAPIC_IOWIN);
//}

static void ioapic_write(uint32_t reg, uint32_t val) {
    *ioapic_addr(IOAPIC_IOREGSEL) = reg;
    *ioapic_addr(IOAPIC_IOWIN) = val;
}

static uint32_t apic_get_id(void) {
    /* In x2APIC mode the APIC ID is the low 32 bits of the APICID MSR. */
    return (uint32_t)(rdmsr(X2APIC_MSR_APICID) & 0xFFFFFFFFULL);
}

static void ioapic_route_pin(uint8_t pin, uint8_t vector) {
    uint32_t reg_low = IOAPIC_REDIR_BASE + (2U * (uint32_t)pin);
    uint32_t reg_high = reg_low + 1;

    uint32_t low = (uint32_t)vector
                 | (0U << IOAPIC_REDIR_DEST_MODE_SHIFT)  /* fixed          */
                 | (0U << IOAPIC_REDIR_DEST_PHYS_SHIFT)  /* physical       */
                 | (0U << IOAPIC_REDIR_POLARITY_SHIFT)   /* active high    */
                 | (0U << IOAPIC_REDIR_TRIGGER_SHIFT)    /* edge triggered */
                 | (0U << IOAPIC_REDIR_MASK_SHIFT);      /* unmasked       */

    /*
     * The destination field lives in bits 31:24 of the high half. Writing 0
     * here worked only because the BSP happens to be APIC 0 under OVMF; on any
     * firmware that boots us on another CPU the keyboard interrupts were
     * routed into the void.
     */
    uint32_t high = (apic_get_id() & 0xFFU) << IOAPIC_REDIR_DEST_ID_SHIFT;

    ioapic_write(reg_low, low);
    ioapic_write(reg_high, high);
}

void apic_init(void) {
    if (!cpuid_has_x2apic()) {
        panic("x2APIC is not supported by this CPU");
    }

    uint64_t base = rdmsr(IA32_APIC_BASE_MSR);

    /*
     * Left exactly as it was. The architecturally tidy version of this
     * (clear bit 11, set bit 10, done) is not obviously better on real
     * firmware and it is not what this kernel was booting with, so it stays
     * untouched until the enable sequence gets validated on its own.
     */
    base &= ~((uint64_t)1 << 11);
    base |= (uint64_t)1 << 10 | (uint64_t)1 << 11;
    wrmsr(IA32_APIC_BASE_MSR, base);

    /* LINT0/LINT1 are the two 8259 outputs: mask both, nothing is wired. */
    x2apic_write(X2APIC_MSR_LVT_LINT0, APIC_LVT_MASKED);
    x2apic_write(X2APIC_MSR_LVT_LINT1, APIC_LVT_MASKED);

    /*
     * LVT Error used to point at APIC_TIMER_VECTOR + 3, which is IRQ 35's
     * vector. An APIC error therefore ran the keyboard handler. Mask it for
     * now; unmasking needs a vector of its own, since everything in 32..47 is
     * already spoken for.
     */
    x2apic_write(X2APIC_MSR_LVT_ERROR, APIC_LVT_MASKED);

    /* Enable the APIC and point its spurious vector at a real IDT gate. */
    x2apic_write(X2APIC_MSR_SPIVR, APIC_SPURIOUS_VEC | (1 << 8));

    outb(PIC1_CMD, 0xFF);
    outb(PIC2_CMD, 0xFF);

    vmm_map_mmio(IOAPIC_VIRT, IOAPIC_BASE);
    ioapic_route_pin(1, KB_IRQ_VECTOR);
}

void apic_send_eoi(void) {
    x2apic_write(X2APIC_MSR_EOI, 0);
}

void apic_timer_init(uint32_t tsc_freq) {
    /*
     * LVT Timer bit 16 masks the timer and bit 17 makes it periodic, so the
     * three writes below have to happen in this order: mask it, point it at
     * the right vector, load a divider and a count, then unmask. Writing an
     * unmasked timer before TMRINITCNT means the first period is whatever was
     * left in the counter, which on a freshly reset APIC is 0 and produces a
     * storm of interrupts.
     */
    x2apic_write(X2APIC_MSR_LVT_TIMER, APIC_TIMER_VECTOR | APIC_LVT_MASKED | APIC_LVT_PERIODIC);

    /* Divide by 16. */
    x2apic_write(X2APIC_MSR_TMRDIV, 0x03);

    /*
     * The counter decrements at tsc_freq / 16, so one millisecond of ticks is
     * tsc_freq / 16 / 1000 = tsc_freq / 16000. sleep_ms() treats one
     * interrupt as one millisecond, which only holds if this is right.
     */
    uint32_t ticks_per_ms = tsc_freq / (16 * 1000);
    if (ticks_per_ms == 0) {
        panic("apic_timer_init: tsc frequency too low for a 1ms period");
    }
    x2apic_write(X2APIC_MSR_TMRINITCNT, ticks_per_ms);

    /* Periodic, unmasked. */
    x2apic_write(X2APIC_MSR_LVT_TIMER, APIC_TIMER_VECTOR | APIC_LVT_PERIODIC);
}

void apic_timer_irq(void) {
    apic_ticks++;
}

uint32_t apic_get_ticks(void) {
    return apic_ticks;
}
