#pragma once
#include "stdint.h"
// logger.h

typedef enum {
    LOG_INFO,
    LOG_WARN,
    LOG_ERROR,
    LOG_DEBUG
} log_level_t;

void kclear(uint32_t color);
void kprintf(const char* text);
void kputc(const char c);
void draw_cursor(void);
void erase_cursor(void);
uint64_t get_cursor_x(void);
uint64_t get_cursor_y(void);
void set_cursor(uint64_t x, uint64_t y);
void klog(log_level_t level, const char *message);
