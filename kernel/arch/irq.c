#include "pinx/irq.h"
#include "pinx/apic.h"
#include "pinx/terminal.h"
#include "pinx/keyboard.h"
#include <stdint.h>

struct interrupt_frame {
    uint64_t r15, r14, r13, r12, r11, r10, r9, r8;
    uint64_t rbp, rdi, rsi, rdx, rcx, rbx, rax;
    uint64_t vector;
    uint64_t error_code;
};

void irq_common_handler(struct interrupt_frame *frame) {
    uint64_t vector = frame->vector;

    if (vector == APIC_TIMER_VECTOR) {
        apic_timer_irq();
    } else if (vector == KB_IRQ_VECTOR) {
        keyboard_irq();
    }

    apic_send_eoi();
}
