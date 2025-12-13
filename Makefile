# Makefile for mise

# Binary name
BINARY_NAME := mise

# Install directory
INSTALL_DIR := $(HOME)/bin

# Cargo flags
CARGO_FLAGS := --release

.PHONY: all build release install clean test check fmt lint help ci ci-quick ci-full coverage

# Default target
all: build

# Build debug version
build:
	cargo build

# Build release version
release:
	cargo build $(CARGO_FLAGS)

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

# Run tests with coverage (requires cargo-tarpaulin)
coverage:
	@command -v cargo-tarpaulin >/dev/null 2>&1 || { echo "Installing cargo-tarpaulin..."; cargo install cargo-tarpaulin; }
	cargo tarpaulin --out Html --out Json --output-dir target/tarpaulin --all-features --ignore-tests
	@echo "Coverage report: target/tarpaulin/tarpaulin-report.html"

# Run tests with coverage and open report
coverage-open: coverage
	@open target/tarpaulin/tarpaulin-report.html 2>/dev/null || xdg-open target/tarpaulin/tarpaulin-report.html 2>/dev/null || echo "Open target/tarpaulin/tarpaulin-report.html manually"

# CI: Standard CI check (fmt + lint + check + test + build)
ci:
	./ci.sh

# CI: Quick check (fmt + lint + check)
ci-quick:
	./ci.sh quick

# CI: Full check (all + fulltest)
ci-full:
	./ci.sh full

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
	@echo "  coverage     Run tests with coverage report"
	@echo "  coverage-open Run coverage and open HTML report"
	@echo "  ci           Run standard CI checks"
	@echo "  ci-quick     Run quick CI checks (no test/build)"
	@echo "  ci-full      Run full CI checks (including fulltest)"
	@echo "  help         Show this help message"
