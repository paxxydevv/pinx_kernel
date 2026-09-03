# Pinx

A bare-metal x86-64 operating system kernel built from scratch using C and x86 assembly. Boots via the [Limine](https://limine-bootloader.org/) bootloader on UEFI systems.

## Features

- GDT
- IDT [WIP]
- PMM
- VMM
- KMALLOC [WIP]
- KFREE [WIP]
- PRITING TO THE FRAMEBUFFER
- UEFI boot

## Prerequisites

Install the following packages on your system:

```bash
# Arch Linux
sudo pacman -S base-devel gcc nasm xorriso qemu-full edk2-ovmf

# Debian / Ubuntu
sudo apt install build-essential nasm xorriso qemu-system-x86 ovmf

# Fedora
sudo dnf groupinstall "Development Tools"
sudo dnf install nasm xorriso qemu-system-x86 edk2-ovmf
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

| Command        | Description                          |
| -------------- | ------------------------------------ |
| `make`         | Build the kernel and create ISO      |
| `make kernel`  | Build only the kernel ELF            |
| `make iso`     | Build kernel and create bootable ISO |
| `make run`     | Build and run in QEMU (UEFI)         |
| `make clean`   | Remove build artifacts               |

## License

See individual files for licensing information.
