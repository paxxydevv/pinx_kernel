/*
 * Log level plumbing.
 *
 * Deliberately free of graphics and console dependencies: nothing here touches
 * the framebuffer, so the whole module compiles and runs on a host, which is
 * what lets the threshold and parsing rules be tested without booting the
 * kernel. The half that does need the console -- printing a tag and calling
 * kvprintf -- stays in graphics/terminal.c, next to the cursor.
 *
 * kdebug.h holds the klog_level_t enum, the K* macros and the prototypes.
 */

#include "pinx/kdebug.h"

#include <stddef.h>

/*
 * The active threshold. volatile because it is meant to be poked from a
 * debugger or a future shell, and because the read inside klog_enabled() must
 * not be hoisted into a register at -O2 and then go stale.
 *
 * Initialised to KLOG_INFO, which matches CONFIG_DEFAULT_LOG_LEVEL, so a build
 * that never calls klog_init() still behaves sensibly.
 */
volatile int klog_threshold = (int)KLOG_INFO;

/* strcmp, which the freestanding build does not have. */
static bool kstreq(const char *a, const char *b) {
    while (*a != '\0' && *a == *b) {
        a++;
        b++;
    }
    return *a == *b;
}

/*
 * The one place the level names are spelled out. The order matches the enum, so
 * a level can be cast to an index if a table ever needs to.
 */
static const struct {
    const char *name;
    klog_level_t level;
} klog_names[] = {
    { "trace", KLOG_TRACE },
    { "debug", KLOG_DEBUG },
    { "info",  KLOG_INFO  },
    { "warn",  KLOG_WARN  },
    { "error", KLOG_ERROR },
    { "none",  KLOG_NONE  },
};

#define KLOG_NAME_COUNT (sizeof(klog_names) / sizeof(klog_names[0]))

bool klog_level_parse(const char *name, klog_level_t *out) {
    if (name == NULL) {
        return false;
    }

    for (size_t i = 0; i < KLOG_NAME_COUNT; i++) {
        if (kstreq(name, klog_names[i].name)) {
            if (out != NULL) {
                *out = klog_names[i].level;
            }
            return true;
        }
    }
    return false;
}

const char *klog_level_name(klog_level_t level) {
    /* KLOG_NONE is in the table, but report the sentinel as "none" explicitly
     * rather than trusting the caller to pass a real level. */
    for (size_t i = 0; i < KLOG_NAME_COUNT; i++) {
        if (klog_names[i].level == level) {
            return klog_names[i].name;
        }
    }
    return "unknown";
}

void klog_set_level(klog_level_t level) {
    klog_threshold = (int)level;
}

klog_level_t klog_get_level(void) {
    return (klog_level_t)klog_threshold;
}

void klog_init(void) {
    klog_level_t level;

    /*
     * A bad CONFIG_DEFAULT_LOG_LEVEL cannot happen through ./configure, which
     * constrains the value to the same word list this parses. Parsing the
     * string at run time rather than trusting a generated integer keeps the
     * kernel from depending on ./configure's internal numbering, and the
     * fallback leaves an unconfigured or hand-edited build at info instead of
     * dumping every trace line.
     */
    if (klog_level_parse(CONFIG_DEFAULT_LOG_LEVEL, &level)) {
        klog_threshold = (int)level;
    } else {
        klog_threshold = (int)KLOG_INFO;
    }
}
