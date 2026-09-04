#pragma once

#include <stdint.h>

#define X2APIC_MSR_BASE 0x800

#define X2APIC_MSR_APICID       (X2APIC_MSR_BASE + 0x02)
#define X2APIC_MSR_LDR          (X2APIC_MSR_BASE + 0x03)
#define X2APIC_MSR_DFR          (X2APIC_MSR_BASE + 0x0E)
#define X2APIC_MSR_SPIVR        (X2APIC_MSR_BASE + 0x0F)
#define X2APIC_MSR_ISR0         (X2APIC_MSR_BASE + 0x10)
#define X2APIC_MSR_TMR          (X2APIC_MSR_BASE + 0x32)
#define X2APIC_MSR_ICR          (X2APIC_MSR_BASE + 0x30)
#define X2APIC_MSR_LVT_TIMER   (X2APIC_MSR_BASE + 0x32)
#define X2APIC_MSR_LVT_LINT0   (X2APIC_MSR_BASE + 0x35)
#define X2APIC_MSR_LVT_LINT1   (X2APIC_MSR_BASE + 0x36)
#define X2APIC_MSR_LVT_ERROR   (X2APIC_MSR_BASE + 0x37)
#define X2APIC_MSR_TMRINITCNT  (X2APIC_MSR_BASE + 0x38)
#define X2APIC_MSR_TMRCURRCNT  (X2APIC_MSR_BASE + 0x39)
#define X2APIC_MSR_TMRDIV      (X2APIC_MSR_BASE + 0x3E)
#define X2APIC_MSR_EOI         (X2APIC_MSR_BASE + 0x0B)

#define APIC_ENABLE_BIT     (1 << 11)
#define X2APIC_ENABLE_BIT   (1 << 10)
#define IA32_APIC_BASE_MSR  0x1B

#define APIC_TIMER_VECTOR  32
#define APIC_SPURIOUS_VEC  0xFF

void apic_init(void);
void apic_send_eoi(void);
void apic_timer_init(uint32_t tsc_freq);
void apic_timer_irq(void);
uint32_t apic_get_ticks(void);
