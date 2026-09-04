#include "../includes/pinx/terminal.h"
#include "../includes/pinx/graphics/graphics.h"
static uint64_t cursor_x = 0;
static uint64_t cursor_y = 0;
void kclear(uint32_t color) {
    cursor_x = 0;
    cursor_y = 0;
    clear_screen(color);
}
static void kputc(const char c) {
    if (c == '\n') {
        cursor_x = 0;
        cursor_y = cursor_y + 30;
        return;
    }
    draw_char(c, cursor_x, cursor_y, 0xFFFFF);
    cursor_x = cursor_x + 8;
}
void kprintf(const char *text) {
    while (*text != '\0') {
        kputc(*text);
        text++;
    }
}
