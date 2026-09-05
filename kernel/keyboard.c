#include "pinx/keyboard.h"
#include "pinx/io.h"
#include "pinx/terminal.h"
#include <stdint.h>

static volatile uint8_t kb_buffer[KB_BUFFER_SIZE];
static volatile uint32_t kb_head = 0;
static volatile uint32_t kb_tail = 0;

static uint8_t shift_held = 0;
static uint8_t ctrl_held = 0;
static uint8_t alt_held = 0;
static uint8_t caps_lock = 0;

static uint8_t extended_prefix = 0;

static const char scancode_to_ascii[128] = {
    KEY_NONE, KEY_ESC,
    '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '-', '=', KEY_BACKSPACE,
    KEY_TAB,
    'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p', '[', ']', KEY_ENTER,
    KEY_NONE,
    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', ';', '\'', '`',
    KEY_NONE,
    '\\', 'z', 'x', 'c', 'v', 'b', 'n', 'm', ',', '.', '/',
    KEY_NONE,
    '*', KEY_NONE, KEY_SPACE, KEY_NONE,
    KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE,
    '7', '8', '9', '-', '4', '5', '6', '+', '1', '2', '3', '0', '.',
    KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE,
};

static const char scancode_to_ascii_shift[128] = {
    KEY_NONE, KEY_ESC,
    '!', '@', '#', '$', '%', '^', '&', '*', '(', ')', '_', '+', KEY_BACKSPACE,
    KEY_TAB,
    'Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P', '{', '}', KEY_ENTER,
    KEY_NONE,
    'A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', ':', '"', '~',
    KEY_NONE,
    '|', 'Z', 'X', 'C', 'V', 'B', 'N', 'M', '<', '>', '?',
    KEY_NONE,
    '*', KEY_NONE, KEY_SPACE, KEY_NONE,
    KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE,
    '7', '8', '9', '-', '4', '5', '6', '+', '1', '2', '3', '0', '.',
    KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE, KEY_NONE,
};

static const uint8_t extended_to_key[128] = {
    [0x48] = KEY_EXT_UP,
    [0x50] = KEY_EXT_DOWN,
    [0x4B] = KEY_EXT_LEFT,
    [0x4D] = KEY_EXT_RIGHT,
    [0x47] = KEY_EXT_HOME,
    [0x4F] = KEY_EXT_END,
    [0x49] = KEY_EXT_PGUP,
    [0x51] = KEY_EXT_PGDN,
    [0x52] = KEY_EXT_INSERT,
    [0x53] = KEY_EXT_DELETE,
    [0x3B] = KEY_EXT_F1,
    [0x3C] = KEY_EXT_F2,
    [0x3D] = KEY_EXT_F3,
    [0x3E] = KEY_EXT_F4,
    [0x3F] = KEY_EXT_F5,
    [0x40] = KEY_EXT_F6,
    [0x41] = KEY_EXT_F7,
    [0x42] = KEY_EXT_F8,
    [0x43] = KEY_EXT_F9,
    [0x44] = KEY_EXT_F10,
    [0x57] = KEY_EXT_F11,
    [0x5D] = KEY_EXT_F12,
    [0x1D] = KEY_NONE,
    [0x38] = KEY_NONE,
    [0x5B] = KEY_NONE,
};

static void kb_buffer_push(uint8_t key) {
    uint32_t next = (kb_head + 1) % KB_BUFFER_SIZE;
    if (next != kb_tail) {
        kb_buffer[kb_head] = key;
        kb_head = next;
    }
}

static uint8_t apply_caps_lock(uint8_t c, uint8_t is_alpha) {
    if (!caps_lock)
        return c;
    if (!is_alpha)
        return c;
    if (c >= 'a' && c <= 'z')
        return (uint8_t)(c - 32);
    if (c >= 'A' && c <= 'Z')
        return (uint8_t)(c + 32);
    return c;
}

static void kbd_cmd(uint8_t cmd) {
    for (uint32_t i = 0; i < 1000; i++) {
        if (!(inb(KB_CMD_PORT) & 0x02)) {
            outb(KB_CMD_PORT, cmd);
            return;
        }
        io_wait();
    }
}

static void kbd_write(uint8_t val) {
    for (uint32_t i = 0; i < 1000; i++) {
        if (!(inb(KB_CMD_PORT) & 0x02)) {
            outb(KB_DATA_PORT, val);
            return;
        }
        io_wait();
    }
}

static uint8_t kbd_read(void) {
    for (uint32_t i = 0; i < 1000; i++) {
        if (inb(KB_CMD_PORT) & 0x01) {
            return inb(KB_DATA_PORT);
        }
        io_wait();
    }
    return 0;
}

static void kbd_flush(void) {
    while (inb(KB_CMD_PORT) & 0x01) {
        inb(KB_DATA_PORT);
        io_wait();
    }
}

void keyboard_init(void) {
    kbd_cmd(0xAD);
    kbd_flush();

    kbd_cmd(0x20);
    uint8_t config = kbd_read();
    config |= 0x01;
    config |= 0x40;
    config &= ~0x10u;
    kbd_cmd(0x60);
    kbd_write(config);

    kbd_cmd(0xAE);

    kbd_write(0xF4);
    kbd_read();

    shift_held = 0;
    ctrl_held = 0;
    alt_held = 0;
    caps_lock = 0;
    extended_prefix = 0;
    kb_head = 0;
    kb_tail = 0;
}

void keyboard_irq(void) {
    uint8_t scancode = inb(KB_DATA_PORT);

    if (scancode == 0xE0) {
        extended_prefix = 1;
        return;
    }

    if (extended_prefix) {
        extended_prefix = 0;

        if (scancode == 0x2A || scancode == 0x36) {
            shift_held = 0;
            return;
        }

        if (scancode & 0x80) {
            return;
        }

        uint8_t key = extended_to_key[scancode];
        if (key != KEY_NONE) {
            kb_buffer_push(key);
        }
        return;
    }

    if (scancode == 0x2A || scancode == 0x36) {
        shift_held = 1;
        return;
    }
    if (scancode == 0x1D) {
        ctrl_held = 1;
        return;
    }
    if (scancode == 0x38) {
        alt_held = 1;
        return;
    }

    if (scancode == 0x3A) {
        caps_lock = !caps_lock;
        return;
    }

    if (scancode & 0x80) {
        uint8_t released = scancode & 0x7F;
        if (released == 0x2A || released == 0x36)
            shift_held = 0;
        else if (released == 0x1D)
            ctrl_held = 0;
        else if (released == 0x38)
            alt_held = 0;
        return;
    }

    if (scancode >= 128) {
        return;
    }

    uint8_t c;
    uint8_t is_shift = shift_held;
    uint8_t is_alpha = 0;

    if (scancode >= 0x10 && scancode <= 0x19)
        is_alpha = 1;
    if (scancode >= 0x1E && scancode <= 0x26)
        is_alpha = 1;
    if (scancode >= 0x2C && scancode <= 0x32)
        is_alpha = 1;

    if (is_shift)
        c = (uint8_t)scancode_to_ascii_shift[scancode];
    else
        c = (uint8_t)scancode_to_ascii[scancode];

    if (c == KEY_NONE)
        return;

    if (is_alpha) {
        c = apply_caps_lock(c, 1);
    } else if (caps_lock && is_shift) {
        c = apply_caps_lock(c, 0);
    }

    if (ctrl_held) {
        if (c >= 'a' && c <= 'z') {
            c = c - 'a' + 1;
        } else if (c >= 'A' && c <= 'Z') {
            c = c - 'A' + 1;
        }
    }

    kb_buffer_push((uint8_t)c);
}

uint8_t keyboard_input(void) {
    if (kb_head == kb_tail) {
        return KEY_NONE;
    }
    uint8_t key = (uint8_t)kb_buffer[kb_tail];
    kb_tail = (kb_tail + 1) % KB_BUFFER_SIZE;
    return key;
}

uint8_t keyboard_has_input(void) {
    return kb_head != kb_tail;
}
