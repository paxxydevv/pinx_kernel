
# Build system for the Pinx kernel.
#
# The kernel, the C library and every userspace program are Cargo projects, so
# this file only orchestrates: it drives cargo, stages the userspace binaries
# into build/, builds the bootable ISO with Limine, and assembles the GPT + ext4
# data disk the AHCI driver mounts to load /init.

SHELL := /bin/sh

CARGO    ?= cargo
XORRISO  ?= xorriso
MKFS     ?= mke2fs
SFDISK   ?= sfdisk

# Number of CPUs QEMU gives the guest. The kernel brings up one idle task per
# CPU and preemptively round-robins ring-3 tasks across them.
SMP ?= 4

# Guest RAM.
MEM ?= 2G

# Size of the data disk holding /init and /hello, and the offset (in 1 MiB
# units) where the single GPT partition starts. 2048 sectors is where every
# partitioning tool aligns to by default.
DISK_SIZE ?= 64M
PART_LBA  := 2048
PART_OFF  := 1048576

BUILD_DIR    := build
TARGET_DIR    := target/x86_64-unknown-none/debug
KERNEL        := $(TARGET_DIR)/pinx
ISO_ROOT      := IsoRoot
ISO           := $(BUILD_DIR)/pinx.iso
SATA_IMG      := $(BUILD_DIR)/sata0.img
ROOTFS_DIR    := $(BUILD_DIR)/rootfs
ROOTFS_IMG    := $(BUILD_DIR)/rootfs.ext4

LIMINE_DIR := limine-binary

# Limine's prebuilt bootloader: the UEFI application the ISO boots from and the
# El Torito image that the firmware loads before it. They come from the "binary"
# asset of a GitHub release, so a fresh clone has neither and would fail at the
# ISO step. The fetch target below checks for them first and downloads only what
# is missing, so a tree that already has them never touches the network.
#
# Override with `make LIMINE_VERSION=12.9.3` to pick up a different release.
LIMINE_VERSION ?= 12.8.0
LIMINE_URL     := https://github.com/Limine-Bootloader/Limine/releases/download/v$(LIMINE_VERSION)/limine-binary.zip
LIMINE_FILES   := $(LIMINE_DIR)/BOOTX64.EFI $(LIMINE_DIR)/limine-uefi-cd.bin

# OVMF firmware, whose paths differ between distributions.
#
# QEMU's own edk2 build splits the firmware: the x86_64 descriptor at
# /usr/share/qemu/firmware/60-edk2-x86_64.json pairs edk2-x86_64-code.fd with
# edk2-i386-vars.fd as its NVRAM template (the variable store is
# architecture-independent), so those paths come first.
OVMF_CODE := $(firstword \
	$(wildcard /usr/share/qemu/edk2-x86_64-code.fd) \
	$(wildcard /usr/share/OVMF/OVMF_CODE.fd) \
	$(wildcard /usr/share/OVMF/OVMF_CODE_4M.fd) \
	$(wildcard /usr/share/edk2/ovmf/OVMF_CODE.fd) \
	$(wildcard /usr/share/edk2/x64/OVMF_CODE.4m.fd) \
	$(wildcard /usr/share/edk2/OvmfX64/OVMF_CODE.fd) \
	$(wildcard /usr/share/edk2-ovmf/OVMF_CODE.fd) \
	$(wildcard /usr/share/qemu/ovmf-x86_64-code.bin) \
)

OVMF_VARS := $(firstword \
	$(wildcard /usr/share/qemu/edk2-i386-vars.fd) \
	$(wildcard /usr/share/qemu/edk2-x86_64-vars.fd) \
	$(wildcard /usr/share/OVMF/OVMF_VARS.fd) \
	$(wildcard /usr/share/OVMF/OVMF_VARS_4M.fd) \
	$(wildcard /usr/share/edk2/ovmf/OVMF_VARS.fd) \
	$(wildcard /usr/share/edk2/x64/OVMF_VARS.4m.fd) \
	$(wildcard /usr/share/edk2/OvmfX64/OVMF_VARS.fd) \
	$(wildcard /usr/share/edk2-ovmf/OVMF_VARS.fd) \
	$(wildcard /usr/share/qemu/ovmf-x86_64-vars.bin) \
)

.PHONY: all iso kernel disk init run debug limine-fetch clean distclean help

all: run

# The userspace programs. Each one stages its own binary into build/, where the
# disk target picks it up; they are real targets rather than .PHONY so `make
# disk` does not relink three projects on every invocation.
build/init: force
	@$(MAKE) --no-print-directory -C pinx_init install
	@touch $@

build/hello: force
	@$(MAKE) --no-print-directory -C pinx_hello install
	@touch $@

.PHONY: force
force:

kernel: $(KERNEL)

$(KERNEL): force
	$(CARGO) build

# Download the Limine binaries unless they are already there. Kept as one shell
# command with the guard first so a tree that has them pays only a stat.
limine-fetch:
	@if [ ! -f $(LIMINE_DIR)/BOOTX64.EFI ] || [ ! -f $(LIMINE_DIR)/limine-uefi-cd.bin ]; then \
		command -v curl >/dev/null 2>&1 || command -v wget >/dev/null 2>&1 || { \
			echo "make: need curl or wget to fetch Limine" >&2; exit 1; \
		}; \
		command -v unzip >/dev/null 2>&1 || { \
			echo "make: need unzip to unpack Limine" >&2; exit 1; \
		}; \
		mkdir -p $(BUILD_DIR) $(LIMINE_DIR); \
		echo "  FETCH   Limine v$(LIMINE_VERSION)"; \
		if command -v curl >/dev/null 2>&1; then \
			curl -sSL -o $(BUILD_DIR)/limine-binary.zip "$(LIMINE_URL)" || { \
				echo "make: download of $(LIMINE_URL) failed" >&2; exit 1; \
			}; \
		else \
			wget -q -O $(BUILD_DIR)/limine-binary.zip "$(LIMINE_URL)" || { \
				echo "make: download of $(LIMINE_URL) failed" >&2; exit 1; \
			}; \
		fi; \
		unzip -q -o $(BUILD_DIR)/limine-binary.zip -d $(BUILD_DIR) || exit 1; \
		cp -R $(BUILD_DIR)/limine-binary/. $(LIMINE_DIR)/ || exit 1; \
		rm -rf $(BUILD_DIR)/limine-binary $(BUILD_DIR)/limine-binary.zip; \
		echo "  FETCH   $(LIMINE_FILES)"; \
	fi

# --------------------------------------------------
# Bootable ISO
# --------------------------------------------------

iso: kernel limine-fetch
	@mkdir -p $(ISO_ROOT)/EFI/BOOT $(ISO_ROOT)/boot
	@cp $(KERNEL) $(ISO_ROOT)/boot/kernel.elf
	@cp $(LIMINE_DIR)/BOOTX64.EFI $(ISO_ROOT)/EFI/BOOT/BOOTX64.EFI
	@cp $(LIMINE_DIR)/limine-uefi-cd.bin $(ISO_ROOT)/limine-uefi-cd.bin
	@cp limine.conf $(ISO_ROOT)/limine.conf
	@mkdir -p $(BUILD_DIR)
	@echo "  ISO     $(ISO)"
	@$(XORRISO) -as mkisofs \
		-R \
		-r \
		-J \
		-V PINX \
		-hfsplus \
		-apm-block-size 2048 \
		--efi-boot limine-uefi-cd.bin \
		-efi-boot-part \
		--efi-boot-image \
		--protective-msdos-label \
		-o $(ISO) \
		$(ISO_ROOT) >/dev/null 2>&1
	@echo "  DONE    $(ISO)"

# --------------------------------------------------
# Data disk: GPT + ext4
# --------------------------------------------------

disk: iso build/init build/hello
	@mkdir -p $(ROOTFS_DIR)
	@cp build/init $(ROOTFS_DIR)/init
	@cp build/hello $(ROOTFS_DIR)/hello
	@rm -f $(ROOTFS_IMG)
	@echo "  MKE2FS  $(ROOTFS_IMG) (/init /hello)"
	@$(MKFS) -q -F -t ext4 -L pinx -d $(ROOTFS_DIR) $(ROOTFS_IMG) \
		$$(($(subst M,,$(DISK_SIZE)) - 1))M
	@rm -f $(SATA_IMG)
	@truncate -s $(DISK_SIZE) $(SATA_IMG)
	@echo "  SATA    $(SATA_IMG) ($(DISK_SIZE), GPT + ext4)"
	@printf 'label: gpt\n\nstart=$(PART_LBA), type=0FC63DAF-8483-4772-8E79-3D69D8477DE4, name=pinx\n' \
		| $(SFDISK) -q $(SATA_IMG) >/dev/null 2>&1
	@dd if=$(ROOTFS_IMG) of=$(SATA_IMG) bs=1M seek=1 conv=notrunc status=none

# --------------------------------------------------
# Run
# --------------------------------------------------

# One disk behind the AHCI controller the q35 southbridge already provides.
QEMU_SATA := \
	-drive if=none,id=sata0,file=$(SATA_IMG),format=raw \
	-device ide-hd,drive=sata0,bus=ide.0

run: disk
	@echo "  RUN     Pinx in QEMU ($(SMP) cpus)"
	@cp $(OVMF_VARS) $(BUILD_DIR)/OVMF_VARS.fd
	@qemu-system-x86_64 \
		-M q35 \
		-m $(MEM) \
		-cpu host \
		-enable-kvm \
		-smp $(SMP) \
		-drive if=pflash,format=raw,readonly=on,file=$(OVMF_CODE) \
		-drive if=pflash,format=raw,file=$(BUILD_DIR)/OVMF_VARS.fd \
		-cdrom $(ISO) \
		$(QEMU_SATA)

# Same as run, but stops on the first CPU exception so gdb can show you where
# it happened instead of the kernel spinning in its exception handler.
#
#   make debug                                  # in one terminal
#   gdb -ex 'target remote :1234' $(KERNEL)     # in another
debug: disk
	@echo "  DEBUG   Pinx under gdb on :1234"
	@cp $(OVMF_VARS) $(BUILD_DIR)/OVMF_VARS.fd
	@qemu-system-x86_64 \
		-M q35 \
		-m $(MEM) \
		-cpu host \
		-enable-kvm \
		-smp $(SMP) \
		-s -S \
		-no-reboot \
		-d int,cpu_reset \
		-dfilter 0x1 \
		-drive if=pflash,format=raw,readonly=on,file=$(OVMF_CODE) \
		-drive if=pflash,format=raw,file=$(BUILD_DIR)/OVMF_VARS.fd \
		-cdrom $(ISO) \
		$(QEMU_SATA)

# --------------------------------------------------
# Clean
# --------------------------------------------------

clean:
	@rm -rf $(BUILD_DIR)
	@rm -rf $(ISO_ROOT)/boot $(ISO_ROOT)/EFI
	@$(CARGO) clean

distclean: clean
	@$(CARGO) clean --release
	@rm -f compile_commands.json

help:
	@echo "Pinx kernel (Rust)"
	@echo ""
	@echo "  make          build the disk and boot it in QEMU"
	@echo "  make disk     build the ISO and the GPT/ext4 data disk"
	@echo "  make iso      build the bootable ISO only"
	@echo "  make kernel   cargo build the kernel only"
	@echo "  make init     build and stage the userspace programs"
	@echo "  make run      boot in QEMU"
	@echo "  make debug    boot under gdb on :1234, halting on exceptions"
	@echo "  make clean    remove build output and cargo target"
	@echo ""
	@echo "  SMP=$(SMP)            CPUs given to the guest"
	@echo "  MEM=$(MEM)                 guest RAM"
	@echo "  DISK_SIZE=$(DISK_SIZE)       data disk size"
