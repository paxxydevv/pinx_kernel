#include "pinx/delay.h"
#include "pinx/apic.h"
#include "pinx/io.h"
#include <stdint.h>

void delay_init(uint32_t tsc_freq) {
    apic_timer_init(tsc_freq);
}

void sleep_ms(uint32_t ms) {
    uint64_t flags = irq_save();
    uint32_t start = apic_get_ticks();

    /*
     * HLT only wakes on an interrupt, so interrupts have to be on inside the
     * loop. They go back to however the caller had them on the way out.
     */
    irq_enable();

    /*
     * Unsigned subtraction instead of `ticks < start + ms`, so the compare
     * still behaves across the 32-bit tick counter wrapping. The old form
     * turned a sleep that straddles the wrap into a wait of nearly 2^32 ticks.
     */
    while ((uint32_t)(apic_get_ticks() - start) < ms) {
        __asm__ volatile ("hlt");
    }

    irq_restore(flags);
}
