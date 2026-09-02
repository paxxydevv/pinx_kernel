# Pinx

A bare-metal x86-64 operating system kernel built from scratch using C and x86 assembly. Boots via the [Limine](https://limine-bootloader.org/) bootloader on UEFI systems.

## Features

- GDT
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

## Getting Started

1. Clone the repository:

```bash
git clone https://github.com/paxxydevv/pinx.git
cd pinx
```

2. Download the Limine bootloader binaries:

```bash
git clone https://github.com/limine-bootloader/limine-binary.git --branch latest-binary --depth=1
```

3. Build the kernel and ISO:

```bash
make
```

4. Run in QEMU:

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
