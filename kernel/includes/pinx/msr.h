#pragma once

#include <stdint.h>

static inline uint64_t rdmsr(uint32_t msr) {
    uint32_t low, high;
    __asm__ volatile ("rdmsr" : "=a"(low), "=d"(high) : "c"(msr));
    return ((uint64_t)high << 32) | low;
}

static inline void wrmsr(uint32_t msr, uint64_t val) {
    __asm__ volatile ("wrmsr" : : "a"((uint32_t)val),
                                  "d"((uint32_t)(val >> 32)),
                                  "c"(msr));
}
