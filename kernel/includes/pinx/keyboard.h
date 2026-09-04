#pragma once

#include <stdint.h>

#define KB_DATA_PORT    0x60
#define KB_CMD_PORT     0x64

#define KB_IRQ_VECTOR   33

#define KB_BUFFER_SIZE  256

#define KEY_NONE        0x00
#define KEY_BACKSPACE   0x08
#define KEY_TAB         0x09
#define KEY_ENTER       0x0A
#define KEY_ESC         0x1B
#define KEY_SPACE       0x20

#define KEY_EXT_UP      0x80
#define KEY_EXT_DOWN    0x81
#define KEY_EXT_LEFT    0x82
#define KEY_EXT_RIGHT   0x83
#define KEY_EXT_HOME    0x84
#define KEY_EXT_END     0x85
#define KEY_EXT_PGUP    0x86
#define KEY_EXT_PGDN    0x87
#define KEY_EXT_INSERT  0x88
#define KEY_EXT_DELETE  0x89
#define KEY_EXT_F1      0x8A
#define KEY_EXT_F2      0x8B
#define KEY_EXT_F3      0x8C
#define KEY_EXT_F4      0x8D
#define KEY_EXT_F5      0x8E
#define KEY_EXT_F6      0x8F
#define KEY_EXT_F7      0x90
#define KEY_EXT_F8      0x91
#define KEY_EXT_F9      0x92
#define KEY_EXT_F10     0x93
#define KEY_EXT_F11     0x94
#define KEY_EXT_F12     0x95

void keyboard_init(void);
void keyboard_irq(void);
uint8_t keyboard_input(void);
uint8_t keyboard_has_input(void);
