#include "../includes/pinx/terminal.h"
#include "../includes/pinx/graphics/graphics.h"
#include <stdarg.h>
static uint64_t cursor_x = 0;
static uint64_t cursor_y = 0;

#define LINE_HEIGHT 30
#define FONT_WIDTH 8
#define SCROLL_COLOR 0x000000

static void terminal_newline(void) {
    cursor_x = 0;
    cursor_y = cursor_y + LINE_HEIGHT;
    if (cursor_y >= get_framebuffer_height()) {
        scroll_framebuffer(LINE_HEIGHT, SCROLL_COLOR);
        cursor_y = get_framebuffer_height() - LINE_HEIGHT;
    }
}
static void terminal_advance(void) {
    cursor_x = cursor_x + FONT_WIDTH;
    if (cursor_x > get_framebuffer_width() - FONT_WIDTH) {
        terminal_newline();
    }
}
void kclear(uint32_t color) {
    cursor_x = 0;
    cursor_y = 0;
    clear_screen(color);
}

void kputc(const char c) {
    if (c == '\n') {
        terminal_newline();
        return;
    }
    draw_char(c, cursor_x, cursor_y, 0xFFFFFF);
    terminal_advance();
}
void kputc_color(const char c, uint32_t color) {
    if (c == '\n') {
        terminal_newline();
        return;
    }
    draw_char(c, cursor_x, cursor_y, color);
    terminal_advance();
}
void kprintf(const char *text) {
    while (*text != '\0') {
        kputc(*text);
        text++;
    }
}
void kprintf_color(const char *text, uint32_t color) {
    while (*text != '\0') {
        kputc_color(*text, color);
        text++;
    }
}

/*
 * Minimal printf, because there is no libc here. Supports %d %i %u %x %X %s
 * %c %p %%, an optional '0' flag, a decimal width, and the l / ll / z length
 * modifiers. That covers everything the kernel logs; it is not a general
 * printf and does not try to be.
 */
static void kputn(char c, int count) {
    while (count-- > 0) {
        kputc(c);
    }
}

static void kput_uint(uint64_t value, uint32_t base, bool upper,
                      int width, bool zero_pad) {
    static const char lower_digits[] = "0123456789abcdef";
    static const char upper_digits[] = "0123456789ABCDEF";
    const char *digits = upper ? upper_digits : lower_digits;

    char buf[24];
    int len = 0;

    if (value == 0) {
        buf[len++] = '0';
    }
    while (value != 0) {
        buf[len++] = digits[value % base];
        value /= base;
    }

    int pad = (width > len) ? width - len : 0;
    kputn(zero_pad ? '0' : ' ', pad);

    while (len-- > 0) {
        kputc(buf[len]);
    }
}

static void kput_int(int64_t value, uint32_t base, bool upper,
                     int width, bool zero_pad) {
    if (value < 0) {
        kputc('-');
        /* The sign consumed one column of the requested width. Negate through
         * unsigned so INT64_MIN does not overflow. */
        kput_uint(0ULL - (uint64_t)value, base, upper,
                  (width > 0) ? width - 1 : 0, zero_pad);
        return;
    }
    kput_uint((uint64_t)value, base, upper, width, zero_pad);
}

/*
 * Note on style: the argument fetch below is written out inline per
 * conversion rather than hidden in a `va_list *` helper. That is deliberate.
 * va_list is an array type on x86-64 and a plain pointer on i386, so a helper
 * taking `va_list *` is a compile error on 64-bit while building fine for this
 * 32-bit target. Calling va_arg directly in the function that received the
 * va_list is the one form that is correct on every ABI.
 */

void kvprintf(const char *fmt, va_list args) {
    for (const char *p = fmt; *p != '\0'; p++) {
        if (*p != '%') {
            kputc(*p);
            continue;
        }

        p++;
        if (*p == '%') {
            kputc('%');
            continue;
        }

        bool zero_pad = false;
        int width = 0;

        if (*p == '0') {
            zero_pad = true;
            p++;
        }
        while (*p >= '0' && *p <= '9') {
            width = width * 10 + (*p - '0');
            p++;
        }

        /* 0 = int, 1 = long, 2 = long long, 3 = size_t. `z` is kept separate
         * from `ll` on purpose: on this 32-bit target size_t is 4 bytes, so
         * reading it as a long long would pull in 4 bytes of unrelated stack. */
        int lmod = 0;
        while (*p == 'l') {
            lmod++;
            p++;
        }
        if (lmod > 2) {
            lmod = 2;
        }
        if (*p == 'z') {
            lmod = 3;
            p++;
        }

        char spec = *p;

        /* Each conversion pulls its own argument, at the width the caller
         * actually passed. Reading one argument up front for every conversion
         * would corrupt %s and %c, which do not take an integer at all. */
        switch (spec) {
            case 'd':
            case 'i':
                if (lmod == 0) {
                    kput_int(va_arg(args, int), 10, false, width, zero_pad);
                } else if (lmod == 1) {
                    kput_int(va_arg(args, long), 10, false, width, zero_pad);
                } else if (lmod == 2) {
                    kput_int(va_arg(args, long long), 10, false, width, zero_pad);
                } else {
                    kput_int((int64_t)va_arg(args, __SIZE_TYPE__),
                             10, false, width, zero_pad);
                }
                break;
            case 'u':
            case 'x':
            case 'X': {
                uint32_t base = (spec == 'u') ? 10 : 16;
                bool upper = (spec == 'X');
                if (lmod == 0) {
                    kput_uint(va_arg(args, unsigned int), base, upper,
                              width, zero_pad);
                } else if (lmod == 1) {
                    kput_uint(va_arg(args, unsigned long), base, upper,
                              width, zero_pad);
                } else if (lmod == 2) {
                    kput_uint(va_arg(args, unsigned long long), base, upper,
                              width, zero_pad);
                } else {
                    kput_uint((uint64_t)va_arg(args, __SIZE_TYPE__), base,
                              upper, width, zero_pad);
                }
                break;
            }
            case 'p':
                /* Always a full pointer, whatever the length modifier said.
                 * Natural width, like libc, rather than a forced 16 digits. */
                kputc('0');
                kputc('x');
                kput_uint((uint64_t)(uintptr_t)va_arg(args, const void *),
                          16, false, width, zero_pad);
                break;
            case 'c':
                /* int, per the standard promotions for a variadic char. */
                kputc((char)va_arg(args, int));
                break;
            case 's': {
                const char *s = va_arg(args, const char *);
                if (s == 0) {
                    s = "(null)";
                }
                int slen = 0;
                while (s[slen] != '\0') {
                    slen++;
                }
                kputn(' ', width > slen ? width - slen : 0);
                kprintf(s);
                break;
            }
            default:
                /* Unknown conversion: echo it rather than swallow it. */
                kputc('%');
                kputc(spec);
                break;
        }
    }
}

void kprintf_fmt(const char *fmt, ...) {
    va_list args;
    va_start(args, fmt);
    kvprintf(fmt, args);
    va_end(args);
}
void draw_cursor(void) {
    for (uint64_t i = 0; i < 8; i++) {
        draw_pixel(cursor_x + i, cursor_y + 8, 0xFFFFFF); // Could uses a underline. or whatever
    }
}

void erase_cursor(void) {
    for (uint64_t i = 0; i < 8; i++) {
        draw_pixel(cursor_x + i, cursor_y + 8, 0x000000); // Could use a underline. or whatever
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
void kprintf_hex(uint32_t value)
{
    const char hex[] = "0123456789ABCDEF";
    char buf[11];

    buf[0] = '0';
    buf[1] = 'x';

    for (int i = 0; i < 8; i++) {
        buf[2 + i] = hex[(value >> (28 - i * 4)) & 0xF];
    }

    buf[10] = '\0';

    kprintf(buf);
}

void halt(void)
{
    for (;;) {
        __asm__ volatile ("hlt");
    }
}

void panic(const char *msg)
{
    kprintf("PANIC: ");
    kprintf(msg);
    kprintf("\nSystem halted.\n");
    halt();
}