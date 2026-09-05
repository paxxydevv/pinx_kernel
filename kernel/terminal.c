#include "../includes/pinx/terminal.h"
#include "../includes/pinx/graphics/graphics.h"
static uint64_t cursor_x = 0;
static uint64_t cursor_y = 0;

void kclear(uint32_t color) {
    cursor_x = 0;
    cursor_y = 0;
    clear_screen(color);
}

void kputc(const char c) {
    if (c == '\n') {
        cursor_x = 0;
        cursor_y = cursor_y + 30;
        return;
    }
    draw_char(c, cursor_x, cursor_y, 0xFFFFFF);
    cursor_x = cursor_x + 8;
}

void kprintf(const char *text) {
    while (*text != '\0') {
        kputc(*text);
        text++;
    }
}

void draw_cursor(void) {
    for (uint64_t i = 0; i < 8; i++) {
        draw_pixel(cursor_x + i, cursor_y + 8, 0xFFFFFF); // Could uses a underline.
    }
}

void erase_cursor(void) {
    for (uint64_t i = 0; i < 8; i++) {
        draw_pixel(cursor_x + i, cursor_y + 8, 0x000000); // Could use a underline.
    }
}

uint64_t get_cursor_x(void) {
    return cursor_x;
}

uint64_t get_cursor_y(void) {
    return cursor_y;
}

void set_cursor(uint64_t x, uint64_t y) {
    cursor_x = x;
    cursor_y = y;
}
