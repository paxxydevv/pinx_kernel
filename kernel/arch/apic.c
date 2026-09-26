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

static volatile uint32_t apic_ticks = 0;

static inline void x2apic_write(uint32_t reg, uint32_t val) {
    wrmsr(reg, val);
}

static volatile uint32_t *ioapic_addr(uint32_t reg) {
    return (volatile uint32_t *)(IOAPIC_VIRT + reg);
}

static uint32_t ioapic_read(uint32_t reg) {
    *ioapic_addr(IOAPIC_IOREGSEL) = reg;
    return *ioapic_addr(IOAPIC_IOWIN);
}

static void ioapic_write(uint32_t reg, uint32_t val) {
    *ioapic_addr(IOAPIC_IOREGSEL) = reg;
    *ioapic_addr(IOAPIC_IOWIN) = val;
}

static void ioapic_route_pin(uint8_t pin, uint8_t vector) {
    uint32_t reg_low = 0x10 + (2 * pin);
    uint32_t reg_high = reg_low + 1;

    uint32_t low = vector
                 | (0 << 8)   /* fixed delivery        */
                 | (0 << 11)  /* physical destination  */
                 | (0 << 13)  /* active-high           */
                 | (0 << 15)  /* edge triggered        */
                 | (0 << 16); /* unmasked              */

    ioapic_write(reg_low, low);
    ioapic_write(reg_high, 0); /* destination APIC ID 0 */
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

    vmm_map_mmio(IOAPIC_VIRT, IOAPIC_BASE);
    ioapic_route_pin(1, KB_IRQ_VECTOR);
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
