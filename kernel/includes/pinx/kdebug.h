#pragma once

#include "config.h"
#include "terminal.h"

/*
 * Logging levels.
 *
 * KINFO is the "is the kernel working" channel: GDT: OK, PMM INIT: OK and the
 * rest. It is the one people want gone, so it is the one gated on
 * CONFIG_DEBUG. The macro compiles the call out entirely rather than testing a
 * runtime flag, so a disabled build carries neither the string literals nor the
 * call, and the arguments are not evaluated.
 *
 * KERROR and panic are never compiled out. An error path that prints nothing
 * is worse than one that prints too much.
 */

#if CONFIG_DEBUG
#define KINFO(...) kprintf_fmt(__VA_ARGS__)
#else
#define KINFO(...) ((void)0)
#endif

#if CONFIG_DEBUG_PCI
#define KPCI(...) kprintf_fmt(__VA_ARGS__)
#else
#define KPCI(...) ((void)0)
#endif

#if CONFIG_DEBUG_HEAP
#define KHEAP(...) kprintf_fmt(__VA_ARGS__)
#else
#define KHEAP(...) ((void)0)
#endif

/* Always on. Use for conditions the kernel can plausibly continue past. */
#define KERROR(...) kprintf_fmt(__VA_ARGS__)
