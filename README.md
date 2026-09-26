# Pinx

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

### Build options

Optional debug output is controlled by `./configure`. It reads the options from
`Kconfig` and writes `include/generated/config.h`, which is the single source of
truth at compile time.

```bash
./configure                        # keep current choices, or pick defaults
./configure --help                 # list every option
./configure --disable-DEBUG        # drop all optional output
./configure --enable-DEBUG_HEAP    # heap tracing only
```

Choices persist in `config.cache`, so later invocations only change what you
name. The current state is always visible with `make help`.

| Option            | Default | Effect                                       |
| ----------------- | ------- | -------------------------------------------- |
| `DEBUG`           | `y`     | Status/init messages via `KINFO`             |
| `DEBUG_PCI`       | `y`     | PCI enumeration output via `KPCI`            |
| `DEBUG_HEAP`      | `y`     | Heap growth output via `KHEAP`               |

`DEBUG_PCI` and `DEBUG_HEAP` depend on `DEBUG`; disabling `DEBUG` forces them
off, and re-enabling `DEBUG` will not silently turn them back on.

Turning these off removes the strings and code from the binary, but **errors and
panics are never compiled out**. `KERROR(...)` and `panic(...)` are unconditional
in every configuration, because a kernel that fails silently is worse than a
noisy one. Use `KINFO` for progress output you would rather not see in a release
build.

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
