
CXX := g++
CC  := gcc
LD  := ld.lld

KERNEL_DIR  := kernel
INCLUDE_DIR := $(KERNEL_DIR)/includes
CONFIG_DIR  := include/generated
CONFIG_H    := $(CONFIG_DIR)/config.h
CONFIG_MK   := $(CONFIG_DIR)/config.mk

# The pair, used wherever a build step has to wait for the configuration to
# exist and to be current. Never depend on CONFIG_H alone: the Makefile reads
# config.mk and the compiler reads config.h, and ./configure writes both
# together, so a step that waits for one of them can still see a stale other.
CONFIG_FILES := $(CONFIG_H) $(CONFIG_MK)

BUILD_DIR  := build
ISO_ROOT   := IsoRoot
ISO        := $(BUILD_DIR)/pinx.iso
SATA_IMG   := $(BUILD_DIR)/sata0.img

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
# Which goals need a configuration
# --------------------------------------------------
#
# Read the values ./configure settled on. config.h is useless here: GNU make
# treats every '#define' line as a comment, so the Makefile cannot see any
# value in it. That is why ./configure also writes config.mk.
#
# -include, not include, so a fresh clone still configures itself on first
# build instead of erroring out on a missing file.
#
# The catch is that -include is not just "read it if it is there". Given the
# name of a makefile, make also treats it as a goal to remake, and if the rule
# for that goal can run, it runs it -- before make has decided which target the
# user asked for. So on a fresh clone every invocation would run ./configure,
# including `make clean` and `make help`, and ./configure is interactive. Cleaning
# a tree nobody has configured yet should not open a prompt.
#
# Hence the gate. The list is inverted on purpose: a goal is assumed to need the
# configuration unless it is named here, so a new build target is correct by
# default instead of silently building against no config at all.
#
# The default goal is all, so an empty MAKECMDGOALS has to be spelled out for the
# comparison to mean what it looks like. The comparison is against the whole
# goal list, so `make clean kernel` still configures -- cleaning a tree and
# building in one command has to work.
CONFIG_FREE_GOALS := clean distclean help
GOALS := $(or $(MAKECMDGOALS),all)

ifeq ($(filter $(GOALS),$(CONFIG_FREE_GOALS)),$(GOALS))
  CONFIG_SKIP := 1
endif

ifneq ($(CONFIG_SKIP),1)
  -include $(CONFIG_MK)
endif

# --------------------------------------------------
# Build profile
# --------------------------------------------------

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

# Deliberately no per-option source filtering here, e.g. dropping ahci.c when
# CONFIG_AHCI=0. It is the obvious way to make --disable-AHCI shrink the binary,
# and it is wrong: the source list is computed from config.mk while make parses,
# but config.h is what the compiler reads, and ./configure rewrites the two
# together. A build that starts with only one of the pair missing then disagrees
# with itself -- the list omits ahci.c while the preprocessor still emits the
# call to init_ahci() -- and the failure is a link error far from the cause.
#
# Drivers gate themselves with #if CONFIG_* instead. That is decided by the same
# config.h the rest of the file is compiled against, so there is nothing to skew,
# and a disabled driver still contributes no code and no strings.

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
# neither generated file, so fall back to the Kconfig defaults rather than
# failing and making the first build a two step affair.
#
# Both files are produced by the same ./configure run and have to agree with
# each other, so the rule is written on the pair, not on config.h alone.
#
# That matters because they are consumed at two different times. config.mk is
# -included, so the Makefile reads it while parsing, before any target runs;
# config.h is read by the compiler, later. Regenerating only one of them lets
# the two disagree, and a disagreement is now a link error rather than a
# surprise: with config.mk saying AHCI=0 and config.h saying AHCI=1, the C
# compiler emits a call to init_ahci() and the source list has left ahci.c out
# of the build. Making config.mk a target as well fixes that from both ends --
# make rewrites it, notices an included makefile changed, and re-executes itself
# before compiling anything, so both files are re-read consistently.
#
# This deliberately never overwrites an existing pair. Otherwise editing Kconfig,
# or simply running make twice, would silently discard choices made by
# ./configure. `make distclean` is the way to start over.
#
# The guard is what makes that hold under a forced rebuild. Without it, `make
# -B` re-runs this recipe regardless of the files existing, and since
# ./configure is interactive it would block a build that is supposed to be
# unattended.
#
# This has to be ONE logical line, not three. Make runs each recipe line in a
# separate shell, so an `exit 0` on its own line only ends that line's shell and
# the following lines still run -- which is exactly the bug this is guarding
# against. The backslashes keep it in a single shell, where exit really does skip
# the rest.
#
# The test covers the pair, not $@, because the two are generated together: if
# either is missing the other cannot be trusted.
$(CONFIG_H) $(CONFIG_MK):
	@if [ -f $(CONFIG_H) ] && [ -f $(CONFIG_MK) ]; then exit 0; fi; \
	echo "  CONF    no config.h/config.mk, using Kconfig defaults"; \
	./configure

# Generate compile_commands.json for clangd (editor IntelliSense)
# bear only records compilations that actually run. If everything is already
# built, "make kernel" is a no-op and bear leaves behind an EMPTY
# compile_commands.json, which sends clangd off with a default command line
# that has none of the kernel's include paths -- so every file reports bogus
# errors.
#
# Deleting $(BUILD_DIR) is how the rebuild is forced. Do NOT reach for make -B
# here: -B re-runs *every* rule, including the $(CONFIG_H) rule below, and
# ./configure is interactive, so -B drops the build into a config prompt that
# then hangs. Removing the objects rebuilds exactly the compilations bear needs
# to record and nothing else.
db: $(CONFIG_FILES)
	@rm -f compile_commands.json
	@rm -rf $(BUILD_DIR)
	bear -- make kernel
	@grep -q '"file"' compile_commands.json 2>/dev/null || { \
		echo "make db: compile_commands.json is empty -- clangd will not work" >&2; \
		exit 1; \
	}
	@echo "compile_commands.json: $$(grep -c '"file"' compile_commands.json) entries"

# --------------------------------------------------
# C
# --------------------------------------------------

# $(CONFIG_H) and $(CONFIG_MK) are normal prerequisites, not order-only, on
# purpose: changing an option has to force a recompile of everything. If they
# were order-only, toggling --disable-DEBUG would relink the existing objects
# and change nothing. See the rule for the pair further down.

$(BUILD_DIR)/%.c.o: $(KERNEL_DIR)/%.c $(CONFIG_FILES)
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

$(TARGET): $(OBJECTS) linker.ld $(CONFIG_FILES)
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

# One disk behind the AHCI controller the q35 southbridge already provides.
#
# This exists so there is a real drive to find, not just a controller. QEMU's
# q35 machine already has an ICH9 AHCI controller (PCI 8086:2922, at 00:1f.2),
# so without this the kernel's AHCI branch runs against a controller with
# nothing plugged into it and reports an empty port list.
#
# Deliberately NOT -device ich9-ahci. Adding an explicit controller gives a
# *second* AHCI device, and pci_enumrate.c maps every class 01/06/01 device onto
# the one fixed address ABAR_MMIO -- so the second controller would silently
# overwrite the first one's mapping. Attaching to the existing bus keeps it at
# one device, which is what that code can actually cope with.
#
# bus=ide.0 is the IDE bus the southbridge's AHCI controller exports. Its ABAR
# is BAR5 at config offset 0x24, which is what pci_enumrate.c reads.
#
# The disk is a blank file on purpose. Nothing in the kernel reads from it yet
# -- it just has to exist so the controller reports a real LBA range.
QEMU_SATA := \
	-drive if=none,id=sata0,file=$(SATA_IMG),format=raw \
	-device ide-hd,drive=sata0,bus=ide.0

# 64 MiB: big enough to look like a real disk, small enough that it costs
# nothing. Unformatted, and nothing mounts it yet.
$(SATA_IMG):
	@mkdir -p $(BUILD_DIR)
	@truncate -s 64M $@
	@echo "  SATA    $@ (64M, blank scratch disk)"

run: iso $(SATA_IMG)
	@echo "  RUN     Pinx in QEMU"

	@cp $(OVMF_VARS) $(BUILD_DIR)/OVMF_VARS.fd
	qemu-system-x86_64 \
		-M q35 \
		-m 2G \
		-cpu host \
		-enable-kvm \
		-drive if=pflash,format=raw,readonly=on,file=$(OVMF_CODE) \
		-drive if=pflash,format=raw,file=$(BUILD_DIR)/OVMF_VARS.fd \
		-cdrom $(ISO) \
		$(QEMU_SATA)

# Same as run, but stops on the first CPU exception so gdb can show you where
# it happened instead of the kernel spinning in its exception handler.
#
#   make debug        # in one terminal
#   gdb -ex 'target remote :1234' build/kernel.elf
#   (gdb) c
debug: iso $(SATA_IMG)
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
		-cdrom $(ISO) \
		$(QEMU_SATA)

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

