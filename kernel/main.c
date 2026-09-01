#include "includes/pinx/gdt.h"
void kmain() {
    gdt_init();
    for (;;) {
        __asm__ volatile("hlt");
    }
}