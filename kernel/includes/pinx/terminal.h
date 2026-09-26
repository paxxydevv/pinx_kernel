#pragma once
#include <stdarg.h>
#include <stdint.h>
// logger.h

typedef enum {
    LOG_INFO,
    LOG_WARN,
    LOG_ERROR,
    LOG_DEBUG
} log_level_t;

void kclear(uint32_t color);

/* Plain, unformatted. Kept because it is the cheapest way to emit a literal. */
void kprintf(const char *text);

/*
 * Formatted output. Supports %d %i %u %x %X %s %c %p %%, a '0' flag, a decimal
 * width, and l / ll / z length modifiers. That is the whole language; anything
 * else is echoed back verbatim rather than silently dropped.
 */
void kprintf_fmt(const char *fmt, ...) __attribute__((format(printf, 1, 2)));
void kvprintf(const char *fmt, va_list args);

void kputc(const char c);
void draw_cursor(void);
void erase_cursor(void);
uint64_t get_cursor_x(void);
uint64_t get_cursor_y(void);
void set_cursor(uint64_t x, uint64_t y);
void kprintf_hex(uint32_t value);

/* Print `msg` and stop the CPU forever. Used for unrecoverable states where
 * continuing would silently corrupt memory (OOM, bogus page tables, ...). */
__attribute__((noreturn)) void panic(const char *msg);

/* Stop the CPU forever without touching the terminal. */
__attribute__((noreturn)) void halt(void);