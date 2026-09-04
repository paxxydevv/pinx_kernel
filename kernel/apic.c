#include "pinx/apic.h"
#include "pinx/cpuid.h"
#include "pinx/msr.h"
#include "pinx/io.h"
#include "pinx/terminal.h"
#include <stdint.h>

static volatile uint32_t apic_ticks = 0;

static inline void x2apic_write(uint32_t reg, uint32_t val) {
    wrmsr(reg, val);
}

static inline uint32_t x2apic_read(uint32_t reg) {
    return (uint32_t)rdmsr(reg);
}

void apic_init(void) {
    if (!cpuid_has_x2apic()) {
        kprintf("APIC: x2APIC not supported\n");
        for (;;) __asm__ volatile ("hlt");
    }

    uint64_t base = rdmsr(IA32_APIC_BASE_MSR);

    base &= ~((uint64_t)1 << 11);
    base |= (uint64_t)1 << 10 | (uint64_t)1 << 11;
    wrmsr(IA32_APIC_BASE_MSR, base);

    x2apic_write(X2APIC_MSR_LVT_LINT0, 0x00010000);
    x2apic_write(X2APIC_MSR_LVT_LINT1, 0x00010000);
    x2apic_write(X2APIC_MSR_LVT_ERROR, APIC_TIMER_VECTOR + 3);

    x2apic_write(X2APIC_MSR_SPIVR, APIC_SPURIOUS_VEC | (1 << 8));

    outb(0x21, 0xFF);
    outb(0xA1, 0xFF);
}

void apic_send_eoi(void) {
    x2apic_write(X2APIC_MSR_EOI, 0);
}

void apic_timer_init(uint32_t tsc_freq) {
    x2apic_write(X2APIC_MSR_LVT_TIMER, APIC_TIMER_VECTOR | (1 << 16) | (1 << 17));

    x2apic_write(X2APIC_MSR_TMRDIV, 0x03);

    uint32_t ticks_per_ms = tsc_freq / 16000;
    x2apic_write(X2APIC_MSR_TMRINITCNT, ticks_per_ms);

    x2apic_write(X2APIC_MSR_LVT_TIMER, APIC_TIMER_VECTOR | (1 << 17));
}

void apic_timer_irq(void) {
    apic_ticks++;
}

uint32_t apic_get_ticks(void) {
    return apic_ticks;
}
