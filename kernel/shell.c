#include "pinx/terminal.h"
#include "pinx/graphics/graphics.h"
#include "pinx/keyboard.h"
#include "pinx/shell.h"
void init_shell() {
    kclear(0x0000);
    kprintf("Pinx Shell:/ ");
    draw_cursor();
}
void run_shell(void)
{
    char buffer[4096];
    uint64_t pos = 0;

    while (1) {
        while (!keyboard_has_input()) {
            __asm__ volatile ("hlt"); // Can safely halt here, Our lapic generates interupts.
        }

        char c = (char)keyboard_input();

        erase_cursor();

        if (c == '\n') {
            buffer[pos] = '\0';

            // execute_command(buffer); // Will be added after a fs.

            pos = 0;
            kprintf("\nPinx Shell:/ ");
            draw_cursor();
            continue;
        }

        if (c == '\b') {
            if (pos > 0) {
                pos--;
                set_cursor(get_cursor_x() - 8, get_cursor_y());
                draw_char(' ', get_cursor_x(), get_cursor_y(), 0xFFFFFF);
                draw_cursor();
            } else {
                draw_cursor();
            }
            continue;
        }

        if (pos < sizeof(buffer) - 1) {
            buffer[pos++] = c;
            kputc(c);
            draw_cursor();
        } else {
            draw_cursor();
        }
    }
}
