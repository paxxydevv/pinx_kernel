#include "pinx/delay.h"
#include "pinx/apic.h"
#include <stdint.h>

void delay_init(uint32_t tsc_freq) {
    apic_timer_init(tsc_freq);
}

void sleep_ms(uint32_t ms) {
    uint32_t target = apic_get_ticks() + ms;
    while (apic_get_ticks() < target) {
        __asm__ volatile ("sti; hlt; cli");
    }
}
