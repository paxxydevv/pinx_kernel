#include <stdint.h>

void exception_divide_error(void)
{
    (void)0;
    for (;;) {
        __asm__ volatile ("hlt");
    }
}