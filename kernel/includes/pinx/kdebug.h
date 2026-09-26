#pragma once

/*
 * The generated header from ./configure wins whenever it exists. On a fresh
 * clone it does not, so fall back to the committed Kconfig defaults rather
 * than failing to parse. See kconfig_defaults.h.
 */
#if __has_include("config.h")
#include "config.h"
#else
#include "kconfig_defaults.h"
#endif

// IWYU pragma: keep
#include "terminal.h"

#include <stdbool.h>

/*
 * Logging.
 *
 * Two independent filters sit in front of the output, and it is worth keeping
 * them apart:
 *
 *   1. Compile time, via the CONFIG_* options. A disabled macro expands to
 *      ((void)0), so the call, the format string and the argument expressions
 *      all vanish from the binary. This is the only way to actually make a
 *      build smaller, and it is what --disable-DEBUG uses.
 *
 *   2. Run time, via klog_threshold. Every enabled level goes through the same
 *      call, which compares against a variable that can be changed at any
 *      point. Turning a level up therefore costs one predictable branch, and
 *      the messages stay resident so raising the threshold needs no rebuild.
 *
 * Ordering is deliberate, and the second depends on the first: CONFIG_DEBUG
 * gates KINFO, KTRACE and KDEBUG, so once DEBUG is off there is no trace or
 * debug output to threshold at all, and the log level is only consulted for
 * info, warn and error.
 *
 * KERROR and panic are never compiled out and never filtered. A build that
 * hides its own failures is worse than a noisy one. The single exception is
 * KLOG_NONE, which silences even errors, and that has to be asked for on
 * purpose.
 */

/* Ordered, so a threshold is a single >= comparison. */
typedef enum {
    KLOG_TRACE = 0,
    KLOG_DEBUG = 1,
    KLOG_INFO  = 2,
    KLOG_WARN  = 3,
    KLOG_ERROR = 4,
    KLOG_NONE  = 5
} klog_level_t;

/*
 * The active threshold. Defined once in terminal.c. volatile because it is
 * meant to be poked from a debugger or a future shell, and the read in
 * klog_enabled must not be hoisted into a register at -O2.
 */
extern volatile int klog_threshold;

static inline bool klog_enabled(klog_level_t level) {
    /* KLOG_NONE is checked first: it is the one setting that silences
     * errors, so it must win over the "errors always print" rule below. */
    if (klog_threshold >= (int)KLOG_NONE) {
        return false;
    }
    return (int)level >= klog_threshold;
}

/* Apply CONFIG_DEFAULT_LOG_LEVEL. Call once, early in kmain. */
void klog_init(void);

/* Change the threshold at runtime; no rebuild needed. */
void klog_set_level(klog_level_t level);
klog_level_t klog_get_level(void);

/* "trace" ... "error" ... "none", and the reverse mapping. */
bool klog_level_parse(const char *name, klog_level_t *out);
const char *klog_level_name(klog_level_t level);

/*
 * Emit one log line: optional [level] tag, then the formatted message.
 *
 * klog_emit deliberately does no threshold check of its own. The K* macros
 * below test first, which keeps the comparison out of the emit path, and
 * KERROR calls it directly precisely because it must not be filtered.
 */
void klog_emit(klog_level_t level, const char *fmt, ...)
    __attribute__((format(printf, 2, 3)));

/*
 * terminal.h is included above for kprintf_fmt and panic, which the macros
 * below expand to. That include is what makes this header self-contained: a
 * caller needs only kdebug.h, and cannot get an undeclared kprintf_fmt by
 * forgetting it. Include-cleaner still reports it as unused because it only
 * tracks symbols referenced outside macro bodies, which is a false positive
 * here.
 */

/* Thresholds a level against klog_threshold, then prints. */
#define KLOG_AT(level, ...)                                    \
    do {                                                       \
        if (klog_enabled(level)) {                             \
            klog_emit((level), __VA_ARGS__);                   \
        }                                                      \
    } while (0)

#if CONFIG_DEBUG
#define KTRACE(...) KLOG_AT(KLOG_TRACE, __VA_ARGS__)
#define KDEBUG(...) KLOG_AT(KLOG_DEBUG, __VA_ARGS__)
#define KINFO(...)  KLOG_AT(KLOG_INFO, __VA_ARGS__)
#else
#define KTRACE(...) ((void)0)
#define KDEBUG(...) ((void)0)
#define KINFO(...)  ((void)0)
#endif

/* Never compiled out. Use for conditions the kernel can continue past. */
#define KERROR(...) klog_emit(KLOG_ERROR, __VA_ARGS__)

/* Kept on their own options so PCI and heap output can be dropped without
 * losing the rest of the debug output. Given a level, so they sit in the same
 * ordering as everything else and honour the runtime threshold. */
#if CONFIG_DEBUG_PCI
#define KPCI(...)  KLOG_AT(KLOG_INFO, __VA_ARGS__)
#else
#define KPCI(...)  ((void)0)
#endif

#if CONFIG_DEBUG_HEAP
#define KHEAP(...) KLOG_AT(KLOG_WARN, __VA_ARGS__)
#else
#define KHEAP(...) ((void)0)
#endif
