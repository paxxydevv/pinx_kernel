# Build the PINX hello program and stage the binary inside the kernel tree,
# where the kernel's Makefile packages it onto the boot disk as /hello.
#
#   make            build (debug) and stage
#   make release    optimised build
#   make clean

CARGO      ?= cargo
KERNEL_DIR ?= ..
STAGE      := $(KERNEL_DIR)/build/hello

PROFILE ?= debug
ifeq ($(PROFILE),release)
CARGO_FLAGS := --release
TARGET_DIR  := release
else
CARGO_FLAGS :=
TARGET_DIR  := debug
endif

BIN := target/x86_64-unknown-none/$(TARGET_DIR)/hello

.PHONY: all build install release clean

all: install

build:
	$(CARGO) build $(CARGO_FLAGS)

install: build
	@mkdir -p $(dir $(STAGE))
	@cp $(BIN) $(STAGE)
	@echo "  HELLO   staged $(STAGE)"

release:
	@$(MAKE) --no-print-directory PROFILE=release install

clean:
	$(CARGO) clean
