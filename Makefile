
CXX := g++
CC  := gcc
LD  := ld.lld

KERNEL_DIR  := kernel
INCLUDE_DIR := $(KERNEL_DIR)/includes
CONFIG_DIR  := include/generated
CONFIG_H    := $(CONFIG_DIR)/config.h
CONFIG_MK   := $(CONFIG_DIR)/config.mk

BUILD_DIR  := build
ISO_ROOT   := IsoRoot
ISO        := $(BUILD_DIR)/pinx.iso

LIMINE_DIR := limine-binary
LIMINE     := $(LIMINE_DIR)/limine

# OVMF (UEFI firmware) paths differ between distributions.
OVMF_CODE := $(firstword \
	$(wildcard /usr/share/OVMF/OVMF_CODE.fd) \
	$(wildcard /usr/share/OVMF/OVMF_CODE_4M.fd) \
	$(wildcard /usr/share/edk2/ovmf/OVMF_CODE.fd) \
	$(wildcard /usr/share/edk2/x64/OVMF_CODE.4m.fd) \
	$(wildcard /usr/share/edk2/OvmfX64/OVMF_CODE.fd) \
	$(wildcard /usr/share/edk2-ovmf/OVMF_CODE.fd) \
	$(wildcard /usr/share/qemu/ovmf-x86_64-code.bin) \
)

OVMF_VARS := $(firstword \
	$(wildcard /usr/share/OVMF/OVMF_VARS.fd) \
	$(wildcard /usr/share/OVMF/OVMF_VARS_4M.fd) \
	$(wildcard /usr/share/edk2/ovmf/OVMF_VARS.fd) \
	$(wildcard /usr/share/edk2/x64/OVMF_VARS.4m.fd) \
	$(wildcard /usr/share/edk2/OvmfX64/OVMF_VARS.fd) \
	$(wildcard /usr/share/edk2-ovmf/OVMF_VARS.fd) \
	$(wildcard /usr/share/qemu/ovmf-x86_64-vars.bin) \
)

TARGET := $(BUILD_DIR)/kernel.elf

# --------------------------------------------------
# Build profile
# --------------------------------------------------
#
# Read the values ./configure settled on. config.h is useless here: GNU make
# treats every '#define' line as a comment, so the Makefile cannot see any
# value in it. That is why ./configure also writes config.mk.
#
# -include, not include, so a fresh clone still configures itself on first
# build instead of erroring out on a missing file.
-include $(CONFIG_MK)

ifeq ($(CONFIG_PROFILE),release)
# Release: optimise, and ship no debug info at all. -fno-omit-frame-pointer is
# already in COMMON_FLAGS and stays on even here, because a frame pointer is
# what makes any crash report readable at all.
PROFILE_FLAGS := -O2
DEBUG_INFO    :=
else
# Debug: no optimisation, so the debugger shows the code you wrote rather than
# something the optimiser rearranged, and full DWARF for gdb.
PROFILE_FLAGS := -O0
DEBUG_INFO    := -g3 -gdwarf-4
endif

# Shown by `make help`, so a wrong-looking build is easy to explain.
PROFILE_NAME := $(if $(CONFIG_PROFILE),$(CONFIG_PROFILE),debug)

# --------------------------------------------------
# Architecture
# --------------------------------------------------

ARCH_FLAGS := \
	-m64 \
	-march=x86-64-v2 \
	-mcmodel=kernel \
	-mno-red-zone \
	-mno-mmx \
	-mno-sse \
	-mno-sse2 \
	-mno-80387 \
	-mno-3dnow

# --------------------------------------------------
# Include paths
# --------------------------------------------------

INCLUDE_FLAGS := \
	-I$(INCLUDE_DIR) \
	-I$(CONFIG_DIR)

# --------------------------------------------------
# Common compiler flags
# --------------------------------------------------

COMMON_FLAGS := \
	-ffreestanding \
	-fno-stack-protector \
	-fno-stack-check \
	-fno-pie \
	-fno-pic \
	-fno-asynchronous-unwind-tables \
	-fno-unwind-tables \
	-fno-omit-frame-pointer \
	-Wall \
	-Wextra \
	-Wpedantic \
	-Wshadow \
	-Wconversion \
	-Wsign-conversion \
	-Wnull-dereference \
	-Wdouble-promotion \
	-Wformat=2 \
	-Werror=return-type

CFLAGS := \
	$(ARCH_FLAGS) \
	$(COMMON_FLAGS) \
	$(PROFILE_FLAGS) \
	$(DEBUG_INFO) \
	$(INCLUDE_FLAGS) \
	-std=c2x

# --------------------------------------------------
# C++ flags
# --------------------------------------------------

CXXFLAGS := \
	$(ARCH_FLAGS) \
	$(COMMON_FLAGS) \
	$(PROFILE_FLAGS) \
	$(DEBUG_INFO) \
	$(INCLUDE_FLAGS) \
	-std=c++2x \
	-fno-exceptions \
	-fno-rtti \
	-fno-threadsafe-statics \
	-fno-use-cxa-atexit \
	-fno-sized-deallocation

# --------------------------------------------------
# Linker flags
# --------------------------------------------------

LDFLAGS := \
	-m elf_x86_64 \
	-nostdlib \
	-no-pie \
	-O2 \
	-T linker.ld

# --------------------------------------------------
# Find source files
#
# Recursively finds all C, C++, and assembly files
# under kernel/, excluding the includes directory.
# --------------------------------------------------

C_SOURCES := $(shell find \
	$(KERNEL_DIR) \
	-type f \
	\( \
		-name '*.c' \
	\) \
	-not -path '$(INCLUDE_DIR)/*')

CPP_SOURCES := $(shell find \
	$(KERNEL_DIR) \
	-type f \
	\( \
		-name '*.cpp' \
		-o -name '*.cc' \
		-o -name '*.cxx' \
	\) \
	-not -path '$(INCLUDE_DIR)/*')

ASM_SOURCES := $(shell find \
	$(KERNEL_DIR) \
	-type f \
	\( \
		-name '*.S' \
		-o -name '*.s' \
		-o -name '*.asm' \
	\) \
	-not -path '$(INCLUDE_DIR)/*')

# --------------------------------------------------
# Convert source paths to object paths
#
# Examples:
#   kernel/main.c                -> build/main.c.o
#   kernel/graphics/graphics.c   -> build/graphics/graphics.c.o
# --------------------------------------------------

C_OBJECTS := \
	$(C_SOURCES:$(KERNEL_DIR)/%.c=$(BUILD_DIR)/%.c.o)

CPP_OBJECTS := \
	$(CPP_SOURCES:$(KERNEL_DIR)/%.cpp=$(BUILD_DIR)/%.cpp.o)

CPP_OBJECTS += \
	$(filter-out $(CPP_OBJECTS), \
		$(CPP_SOURCES:$(KERNEL_DIR)/%.cc=$(BUILD_DIR)/%.cc.o))

CPP_OBJECTS += \
	$(filter-out $(CPP_OBJECTS), \
		$(CPP_SOURCES:$(KERNEL_DIR)/%.cxx=$(BUILD_DIR)/%.cxx.o))

ASM_CAPITAL_OBJECTS := \
	$(patsubst $(KERNEL_DIR)/%.S,$(BUILD_DIR)/%.S.o,$(filter %.S,$(ASM_SOURCES)))

ASM_LOWER_OBJECTS := \
	$(patsubst $(KERNEL_DIR)/%.s,$(BUILD_DIR)/%.s.o,$(filter %.s,$(ASM_SOURCES)))

ASM_ASM_OBJECTS := \
	$(patsubst $(KERNEL_DIR)/%.asm,$(BUILD_DIR)/%.asm.o,$(filter %.asm,$(ASM_SOURCES)))

ASM_OBJECTS := \
	$(ASM_CAPITAL_OBJECTS) \
	$(ASM_LOWER_OBJECTS) \
	$(ASM_ASM_OBJECTS)

# --------------------------------------------------
# All objects
# --------------------------------------------------

OBJECTS := \
	$(CPP_OBJECTS) \
	$(C_OBJECTS) \
	$(ASM_OBJECTS)

# --------------------------------------------------
# Targets
# --------------------------------------------------

.PHONY: all kernel iso run db clean distclean help release relwithdebug

all: iso

kernel: $(TARGET)

# Build-time configuration. Run ./configure to change it; a fresh checkout has
# no config.h, so fall back to the Kconfig defaults rather than failing and
# making the first build a two step affair.
#
# This deliberately never overwrites an existing config.h. Otherwise editing
# Kconfig, or simply running make twice, would silently discard choices made by
# ./configure. `make distclean` is the way to start over.
$(CONFIG_H):
	@echo "  CONF    no $(CONFIG_H), using Kconfig defaults"
	@./configure

# Generate compile_commands.json for clangd (editor IntelliSense)
# bear only records compilations that actually run. If everything is already
# built, "make kernel" is a no-op and bear leaves behind an EMPTY
# compile_commands.json, which sends clangd off with a default command line
# that has none of the kernel's include paths -- so every file reports bogus
# errors. -B forces the rebuild that gives bear something to record, and the
# check below makes a failure loud instead of silently poisoning the editor.
db: $(CONFIG_H)
	@rm -f compile_commands.json
	bear -- make -B kernel
	@grep -q '"file"' compile_commands.json 2>/dev/null || { \
		echo "make db: compile_commands.json is empty -- clangd will not work" >&2; \
		exit 1; \
	}
	@echo "compile_commands.json: $$(grep -c '"file"' compile_commands.json) entries"

# --------------------------------------------------
# C
# --------------------------------------------------

# $(CONFIG_H) is a normal prerequisite, not order-only, on purpose: changing a
# option has to force a recompile of everything. If it were order-only, toggling
# --disable-DEBUG would relink the existing objects and change nothing.
$(BUILD_DIR)/%.c.o: $(KERNEL_DIR)/%.c $(CONFIG_H)
	@mkdir -p $(dir $@)
	@echo "  CC      $<"
	@$(CC) $(CFLAGS) -c $< -o $@

# --------------------------------------------------
# C++
# --------------------------------------------------

$(BUILD_DIR)/%.cpp.o: $(KERNEL_DIR)/%.cpp $(CONFIG_H)
	@mkdir -p $(dir $@)
	@echo "  CXX     $<"
	@$(CXX) $(CXXFLAGS) -c $< -o $@

$(BUILD_DIR)/%.cc.o: $(KERNEL_DIR)/%.cc $(CONFIG_H)
	@mkdir -p $(dir $@)
	@echo "  CXX     $<"
	@$(CXX) $(CXXFLAGS) -c $< -o $@

$(BUILD_DIR)/%.cxx.o: $(KERNEL_DIR)/%.cxx $(CONFIG_H)
	@mkdir -p $(dir $@)
	@echo "  CXX     $<"
	@$(CXX) $(CXXFLAGS) -c $< -o $@

# --------------------------------------------------
# Assembly
#
# .S = preprocessed assembly
# .s = raw assembly
# .asm = raw assembly
# --------------------------------------------------

$(BUILD_DIR)/%.S.o: $(KERNEL_DIR)/%.S
	@mkdir -p $(dir $@)
	@echo "  AS      $<"
	@$(CC) $(ARCH_FLAGS) -fno-pic $(INCLUDE_FLAGS) -c $< -o $@

$(BUILD_DIR)/%.s.o: $(KERNEL_DIR)/%.s
	@mkdir -p $(dir $@)
	@echo "  AS      $<"
	@$(CC) $(ARCH_FLAGS) -fno-pic $(INCLUDE_FLAGS) -c $< -o $@

$(BUILD_DIR)/%.asm.o: $(KERNEL_DIR)/%.asm
	@mkdir -p $(dir $@)
	@echo "  AS      $<"
	@$(CC) $(ARCH_FLAGS) $(INCLUDE_FLAGS) -x assembler -c $< -o $@

# --------------------------------------------------
# Link kernel
# --------------------------------------------------

$(TARGET): $(OBJECTS) linker.ld $(CONFIG_H)
	@mkdir -p $(dir $@)
	@echo "  LD      $@"
	@$(LD) $(LDFLAGS) -o $@ $(OBJECTS)

# --------------------------------------------------
# Prepare UEFI ISO root
# --------------------------------------------------

iso: $(TARGET) limine.conf
	@echo "  ISO     Preparing ISO root"

	@mkdir -p $(ISO_ROOT)/EFI/BOOT
	@mkdir -p $(ISO_ROOT)/boot

	@cp $(TARGET) \
		$(ISO_ROOT)/boot/kernel.elf

	@cp $(LIMINE_DIR)/BOOTX64.EFI \
		$(ISO_ROOT)/EFI/BOOT/BOOTX64.EFI

	@cp $(LIMINE_DIR)/limine-uefi-cd.bin \
		$(ISO_ROOT)/limine-uefi-cd.bin

	@cp limine.conf \
		$(ISO_ROOT)/limine.conf

	@echo "  ISO     Creating $(ISO)"

	@mkdir -p $(BUILD_DIR)

	@xorriso -as mkisofs \
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
		$(ISO_ROOT)

	@echo "  DONE    $(ISO)"

# --------------------------------------------------
# Run with UEFI / OVMF
# --------------------------------------------------

run: iso
	@echo "  RUN     Pinx in QEMU"

	@cp $(OVMF_VARS) $(BUILD_DIR)/OVMF_VARS.fd
	qemu-system-x86_64 \
		-M q35 \
		-m 2G \
		-cpu host \
		-enable-kvm \
		-drive if=pflash,format=raw,readonly=on,file=$(OVMF_CODE) \
		-drive if=pflash,format=raw,file=$(BUILD_DIR)/OVMF_VARS.fd \
		-cdrom $(ISO)

# Same as run, but stops on the first CPU exception so gdb can show you where
# it happened instead of the kernel spinning in its exception handler.
#
#   make debug        # in one terminal
#   gdb -ex 'target remote :1234' build/kernel.elf
#   (gdb) c
debug: iso
	@echo "  DEBUG   Pinx under gdb on :1234"

	@cp $(OVMF_VARS) $(BUILD_DIR)/OVMF_VARS.fd
	qemu-system-x86_64 \
		-M q35 \
		-m 2G \
		-cpu host \
		-enable-kvm \
		-s -S \
		-no-reboot \
		-d int,cpu_reset \
		-dfilter 0x1 \
		-drive if=pflash,format=raw,readonly=on,file=$(OVMF_CODE) \
		-drive if=pflash,format=raw,file=$(BUILD_DIR)/OVMF_VARS.fd \
		-cdrom $(ISO)

# --------------------------------------------------
# Clean
# --------------------------------------------------

clean:
	rm -rf $(BUILD_DIR)
	rm -rf $(ISO_ROOT)/boot
	rm -rf $(ISO_ROOT)/EFI

# Also forgets the build configuration, so the next make starts from the
# Kconfig defaults. Use this after ./configure if you want to start over.
distclean: clean
	rm -rf $(CONFIG_DIR)

# --------------------------------------------------
# Profile shortcuts
# --------------------------------------------------
#
# Convenience wrappers that reconfigure and rebuild in one step. They change
# ./configure's saved choices, so a plain `make` afterwards keeps them.

release:
	@./configure --profile=release
	@$(MAKE) --no-print-directory all

# -O2 with debug messages kept, for when a release build still misbehaves and
# stepping through -O0 code is not what you need.
relwithdebug:
	@./configure --profile=release --enable-DEBUG
	@$(MAKE) --no-print-directory all

# --------------------------------------------------
# Help
# --------------------------------------------------

help:
	@echo "Pinx kernel"
	@echo ""
	@echo "  make            build the ISO (default)"
	@echo "  make kernel     build build/kernel.elf only"
	@echo "  make run        boot the ISO in QEMU"
	@echo "  make debug      boot under gdb on :1234, halting on exceptions"
	@echo "  make release    reconfigure for release, then build"
	@echo "  make db         regenerate compile_commands.json"
	@echo "  make clean      remove build output"
	@echo "  make distclean  also discard ./configure's choices"
	@echo ""
	@echo "  ./configure     choose build options (see ./configure --help)"
	@echo ""
	@echo "Current configuration (profile: $(PROFILE_NAME)):"
	@if [ -f $(CONFIG_H) ]; then \
		sed -n -e 's/^#define \(CONFIG_[A-Z_]*\) \([01]\)$$/  \1 = \2/p' \
		       -e 's/^#define \(CONFIG_[A-Z_]*\) "\(.*\)"$$/  \1 = \2/p' $(CONFIG_H); \
	else \
		echo "  (none yet, run ./configure or let make pick the defaults)"; \
	fi

