#include "pinx/terminal.h"
#include "pinx/graphics/graphics.h"
#include "pinx/keyboard.h"
#include "pinx/shell.h"
void init_shell() {
    kclear(0xFFFF);
    kprintf("Pinx Shell:/ ");
}