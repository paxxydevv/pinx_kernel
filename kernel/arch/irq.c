#include "pinx/irq.h"
#include "pinx/apic.h"
#include "pinx/terminal.h"
#include "pinx/keyboard.h"
#include <stdint.h>

void irq_spurious_handler(struct interrupt_frame *frame) {
    /*
     * Nothing to acknowledge and nothing to service. The EOI is still sent
     * because vector 15 comes from the 8259, which does need one, and a
     * superfluous EOI on the APIC is a no-op on an interrupt that never
     * reached the in-service register.
     */
    (void)frame;
    apic_send_eoi();
}

void irq_common_handler(struct interrupt_frame *frame) {
    uint64_t vector = frame->vector;

    if (vector == APIC_TIMER_VECTOR) {
        apic_timer_irq();
    } else if (vector == KB_IRQ_VECTOR) {
        keyboard_irq();
    }

    apic_send_eoi();
}
