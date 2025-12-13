# Makefile for mise

# Binary name
BINARY_NAME := mise

# Install directory
INSTALL_DIR := $(HOME)/bin

# Cargo flags
CARGO_FLAGS := --release
FEATURES := --features mcp

.PHONY: all build release install clean test check fmt lint help

# Default target
all: build

# Build debug version (with mcp feature)
build:
	cargo build $(FEATURES)

# Build release version (with mcp feature)
release:
	cargo build $(CARGO_FLAGS) $(FEATURES)

# Build and install to ~/bin
install: release
	@mkdir -p $(INSTALL_DIR)
	@cp target/release/$(BINARY_NAME) $(INSTALL_DIR)/$(BINARY_NAME)
	@echo "Installed $(BINARY_NAME) to $(INSTALL_DIR)/$(BINARY_NAME)"

# Uninstall from ~/bin
uninstall:
	@rm -f $(INSTALL_DIR)/$(BINARY_NAME)
	@echo "Removed $(BINARY_NAME) from $(INSTALL_DIR)"

# Clean build artifacts
clean:
	cargo clean

# Run tests
test:
	cargo test

# Run full test suite
fulltest:
	./fulltest.sh

# Check code without building
check:
	cargo check

# Format code
fmt:
	cargo fmt

# Run clippy linter
lint:
	cargo clippy -- -D warnings

# Build with all features
build-all:
	cargo build $(CARGO_FLAGS) --all-features

# Install with all features
install-all: build-all
	@mkdir -p $(INSTALL_DIR)
	@cp target/release/$(BINARY_NAME) $(INSTALL_DIR)/$(BINARY_NAME)
	@echo "Installed $(BINARY_NAME) (all features) to $(INSTALL_DIR)/$(BINARY_NAME)"

# Show help
help:
	@echo "mise Makefile"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  all          Build debug version (default)"
	@echo "  build        Build debug version"
	@echo "  release      Build release version"
	@echo "  install      Build release and install to ~/bin"
	@echo "  install-all  Build with all features and install to ~/bin"
	@echo "  uninstall    Remove binary from ~/bin"
	@echo "  clean        Clean build artifacts"
	@echo "  test         Run tests"
	@echo "  fulltest     Run full test suite"
	@echo "  check        Check code without building"
	@echo "  fmt          Format code"
	@echo "  lint         Run clippy linter"
	@echo "  help         Show this help message"
