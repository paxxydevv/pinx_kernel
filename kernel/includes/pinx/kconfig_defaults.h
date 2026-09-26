#pragma once

/*
 * Fallback defaults, used only when the generated header is absent.
 *
 * include/generated/config.h is produced by ./configure and is deliberately
 * not committed, so on a fresh clone -- or in an editor before ./configure has
 * been run once -- it does not exist. Without this file every translation unit
 * that includes kdebug.h fails to parse, and clangd reports
 * "'config.h' file not found" on all of them.
 *
 * kdebug.h picks this up via __has_include, and the generated header always
 * wins whenever it is present. That means an unconfigured tree behaves exactly
 * like the Kconfig defaults, which is also what a plain `make` produces.
 *
 * Keep the values below in sync with the `default y|n` lines in ./Kconfig.
 */

#define CONFIG_PROFILE "debug"
/* Build profile */
#define CONFIG_DEBUG 1
/* Print kernel debug messages */
#define CONFIG_DEBUG_PCI 1
/* Print every device found during PCI enumeration */
#define CONFIG_DEBUG_HEAP 1
/* Print kernel heap allocation failures */
#define CONFIG_LOG_TAGS 1
/* Tag each log line with its level */
#define CONFIG_DEFAULT_LOG_LEVEL "info"
/* Log level at boot */
