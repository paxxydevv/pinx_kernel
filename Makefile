
CXX := g++
CC  := gcc
LD  := ld

KERNEL_DIR  := kernel
INCLUDE_DIR := $(KERNEL_DIR)/includes

BUILD_DIR  := build
ISO_ROOT   := IsoRoot
ISO        := $(BUILD_DIR)/pinx.iso

LIMINE_DIR := limine-binary
LIMINE     := $(LIMINE_DIR)/limine

TARGET := $(BUILD_DIR)/kernel.elf

# --------------------------------------------------
# Architecture
# --------------------------------------------------

ARCH_FLAGS := \
	-m64 \
	-march=x86-64 \
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
	-I$(INCLUDE_DIR)

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
	-Werror=return-type \
	-O2

# --------------------------------------------------
# C flags
# --------------------------------------------------

CFLAGS := \
	$(ARCH_FLAGS) \
	$(COMMON_FLAGS) \
	$(INCLUDE_FLAGS) \
	-std=c23

# --------------------------------------------------
# C++ flags
# --------------------------------------------------

CXXFLAGS := \
	$(ARCH_FLAGS) \
	$(COMMON_FLAGS) \
	$(INCLUDE_FLAGS) \
	-std=c++23 \
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

.PHONY: all kernel iso run db clean

all: iso

kernel: $(TARGET)

# Generate compile_commands.json for clangd (editor IntelliSense)
db:
	bear -- make kernel

# --------------------------------------------------
# C
# --------------------------------------------------

$(BUILD_DIR)/%.c.o: $(KERNEL_DIR)/%.c
	@mkdir -p $(dir $@)
	@echo "  CC      $<"
	@$(CC) $(CFLAGS) -c $< -o $@

# --------------------------------------------------
# C++
# --------------------------------------------------

$(BUILD_DIR)/%.cpp.o: $(KERNEL_DIR)/%.cpp
	@mkdir -p $(dir $@)
	@echo "  CXX     $<"
	@$(CXX) $(CXXFLAGS) -c $< -o $@

$(BUILD_DIR)/%.cc.o: $(KERNEL_DIR)/%.cc
	@mkdir -p $(dir $@)
	@echo "  CXX     $<"
	@$(CXX) $(CXXFLAGS) -c $< -o $@

$(BUILD_DIR)/%.cxx.o: $(KERNEL_DIR)/%.cxx
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

$(TARGET): $(OBJECTS) linker.ld
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

	qemu-system-x86_64 \
		-M q35 \
		-m 2G \
		-drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/x64/OVMF_CODE.4m.fd \
		-cdrom $(ISO)

# --------------------------------------------------
# Clean
# --------------------------------------------------

clean:
	rm -rf $(BUILD_DIR)
	rm -rf $(ISO_ROOT)/boot
	rm -rf $(ISO_ROOT)/EFI

