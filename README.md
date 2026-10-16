# Pinx
# CRITICAL PROJECT TRANSFORIMING TO RUST BASED ONLY NOW
A bare-metal x86-64 operating system kernel built from scratch using C and x86 assembly. Boots via the [Limine](https://limine-bootloader.org/) bootloader on UEFI systems.

## Features

- GDT
- IDT 
- PMM
- VMM
- KMALLOC
- KFREE
- PRITING TO THE FRAMEBUFFER
- UEFI boot
- LAPIC 
- X2APIC
- FS [WIP]
- DEBUG SHELL[WIP]
## Prerequisites

Install the following packages on your system:

```bash
# Arch Linux
sudo pacman -S clang lld nasm xorriso qemu-full edk2-ovmf

# Debian / Ubuntu
sudo apt install clang lld nasm xorriso qemu-system-x86 ovmf

# Fedora
sudo dnf install clang lld nasm xorriso qemu-system-x86 edk2-ovmf
```

## OVMF (UEFI Firmware for QEMU)

QEMU requires [OVMF](https://github.com/tianocore/edk2) to boot UEFI systems. The Makefile expects it at `/usr/share/edk2/x64/OVMF_CODE.4m.fd`.

**Arch Linux:**
```bash
sudo pacman -S edk2-ovmf
```
OVMF is installed to `/usr/share/edk2/x64/OVMF_CODE.4m.fd` automatically.

**Debian / Ubuntu:**
```bash
sudo apt install ovmf
```
The firmware is installed to `/usr/share/OVMF/OVMF_CODE.fd`. Update the path in the Makefile's `run` target accordingly.

**Fedora:**
```bash
sudo dnf install edk2-ovmf
```
The firmware is installed to `/usr/share/edk2/edk2-x64/OVMF_CODE.fd`. Update the path in the Makefile's `run` target accordingly.

## Getting Started

1. Clone the repository:

```bash
git clone https://github.com/paxxydevv/pinx.git
cd pinx
```


2. Build the kernel and ISO:

```bash
make
```

3. Run in QEMU:

```bash
make run
```
## Building

| Command        | Description                                    |
| -------------- | ---------------------------------------------- |
| `make`         | Build the kernel and create ISO                |
| `make kernel`  | Build only the kernel ELF                      |
| `make iso`     | Build kernel and create bootable ISO           |
| `make run`     | Build and run in QEMU (UEFI)                   |
| `make help`    | List targets and show the current configuration |
| `make clean`   | Remove build artifacts                         |
| `make distclean` | Remove artifacts and discard `./configure` choices |

`make` works on a fresh clone with no configuration step: if `config.h` has not
been generated yet, the defaults in `Kconfig` are used.

`include/generated/config.h` is generated and not committed, so on a fresh clone
it does not exist until `./configure` or `make` runs once. Until then
`kdebug.h` falls back to the committed defaults in
`kernel/includes/pinx/kconfig_defaults.h`, which keeps the tree compiling and
keeps editors and clangd from reporting a missing `config.h`. The generated
header always takes precedence the moment it exists.

`compile_commands.json` is also generated (`make db`) and not committed, so
point your editor's clangd at it after a fresh clone.

### Build options

Build behaviour and debug output are controlled by `./configure`. It reads the
options from `Kconfig` and writes two files: `include/generated/config.h` for
the compiler and `include/generated/config.mk` for the Makefile. Both are
generated; the Makefile needs the second one because `make` treats every
`#define` in the first as a comment.

```bash
./configure                        # keep current choices, or pick defaults
./configure --help                 # list every option
./configure --disable-DEBUG        # drop all optional output
./configure --enable-DEBUG_HEAP    # heap tracing only
./configure --profile=release      # optimised build, no debug info
./configure --log-level=debug      # lower the runtime threshold
```

An option is either named on the command line or it sits at its `Kconfig`
default. Nothing is inherited from a previous run, so the result depends only on
the command line and `Kconfig` — `--enable-DEBUG` never quietly carries a
stale `--disable-LOG_TAGS` along with it. The current state is always visible
with `make help`.

| Option               | Default | Effect                                          |
| -------------------- | ------- | ----------------------------------------------- |
| `PROFILE`            | `debug` | `debug` is `-O0 -g3`; `release` is `-O2`, no debug info |
| `DEBUG`              | `y`     | Status/init messages via `KINFO`                |
| `DEBUG_PCI`          | `y`     | PCI enumeration output via `KPCI`               |
| `DEBUG_HEAP`         | `y`     | Heap growth output via `KHEAP`                  |
| `LOG_TAGS`           | `y`     | Prefix each line with `[info]`, `[warn]`, ...   |
| `DEFAULT_LOG_LEVEL`  | `info`  | Runtime threshold at boot                       |

`make release` reconfigures for `release` and builds in one step;
`make relwithdebug` keeps the output but stays at `-O2`.

### Log levels

There are two independent filters, and the difference matters when sizing a
build:

**Compile time.** A disabled option expands its macro to `((void)0)`, so the
call, the format string and the argument expressions all disappear. This is the
only thing that actually shrinks the binary:

```bash
./configure --disable-DEBUG --disable-DEBUG_PCI --disable-DEBUG_HEAP
```

**Run time.** Every enabled level is compared against a threshold held in
`klog_threshold`, which can be changed at any point with
`klog_set_level()`. Raising it costs one predictable branch and needs no
rebuild, but the strings stay in the binary:

| Level   | Emits                                     |
| ------- | ----------------------------------------- |
| `trace` | `KTRACE`                                  |
| `debug` | `KDEBUG`                                  |
| `info`  | `KINFO`, `KPCI`                           |
| `warn`  | `KHEAP`                                   |
| `error` | `KERROR`                                  |

Level names are `trace`, `debug`, `info`, `warn`, `error` and `none`, ordered
so a threshold is a single comparison. `none` silences everything, errors
included, and has to be asked for deliberately.

`DEBUG_PCI`, `DEBUG_HEAP` and `LOG_TAGS` depend on `DEBUG`; disabling `DEBUG`
forces them off, and re-enabling `DEBUG` will not silently turn them back on.

Turning the compile-time options off removes the strings and code, but **errors
and panics are never compiled out**. `KERROR(...)` and `panic(...)` are
unconditional in every configuration, because a kernel that fails silently is
worse than a noisy one. Use `KINFO` for progress output you would rather not see
in a release build.

## Source Structure

Kernel sources are organized by category:

```text
kernel/
├── core/        Entry point and core scheduling (kmain, scheduler)
├── arch/        x86-64 low-level code (GDT, IDT, IRQ, exceptions, APIC, timer) (I doubt i needed arch dir anyways thinking of making it only x86_64 but in feature maybe a new arch support)
├── mm/          Memory management (PMM, VMM, heap)
├── drivers/     Device drivers (PS/2 keyboard)
├── graphics/    Framebuffer graphics and terminal text output
├── shell/       Debug shell
└── includes/    Headers (pinx/, 3party/)
```

## License

See individual files for licensing information.
