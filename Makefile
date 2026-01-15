.PHONY: all build run test clean fmt clippy install-deps

# Default target
all: build

# Install necessary dependencies
install-deps:
	@echo "Installing bootimage..."
	cargo install bootimage --version "^0.10"
	@echo "Installing dependencies complete"

# Build the kernel
build:
	@echo "Building PandaOS kernel..."
	cargo build --manifest-path kernel/Cargo.toml --target x86_64-unknown-none
	@echo "Build complete!"

# Build release version
release:
	@echo "Building PandaOS kernel (release)..."
	cargo build --manifest-path kernel/Cargo.toml --release --target x86_64-unknown-none
	@echo "Release build complete!"

# Create bootable image
bootimage: build
	@echo "Creating bootable disk image..."
	cargo bootimage --manifest-path kernel/Cargo.toml --target x86_64-unknown-none
	@echo "Bootimage created!"

# Run in QEMU
run: bootimage
	@echo "Starting QEMU..."
	cargo run --manifest-path kernel/Cargo.toml --target x86_64-unknown-none

# Run tests
test:
	@echo "Running host tests..."
	cargo test --lib --workspace --target x86_64-unknown-linux-gnu
	@echo "Running kernel tests..."
	cargo test --manifest-path kernel/Cargo.toml --target x86_64-unknown-none

# Run tests for HAL
test-hal:
	@echo "Running HAL tests..."
	cd hal && cargo test --lib --target x86_64-unknown-linux-gnu

# Run tests for kernel
test-kernel:
	@echo "Running kernel tests..."
	cargo test --manifest-path kernel/Cargo.toml --target x86_64-unknown-none

# Format code
fmt:
	@echo "Formatting code..."
	cargo fmt --all
	@echo "Format complete!"

# Check code formatting
fmt-check:
	@echo "Checking code format..."
	cargo fmt --all -- --check

# Run clippy lints
clippy:
	@echo "Running clippy..."
	cargo clippy --workspace --all-targets -- -D warnings
	@echo "Clippy complete!"

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean
	@echo "Clean complete!"

# Help target
help:
	@echo "PandaOS Makefile targets:"
	@echo "  all         - Build the kernel (default)"
	@echo "  build       - Build the kernel in debug mode"
	@echo "  release     - Build the kernel in release mode"
	@echo "  bootimage   - Create bootable disk image"
	@echo "  run         - Build and run in QEMU"
	@echo "  test        - Run all tests"
	@echo "  test-hal    - Run HAL tests only"
	@echo "  test-kernel - Run kernel tests only"
	@echo "  fmt         - Format code with rustfmt"
	@echo "  fmt-check   - Check code formatting"
	@echo "  clippy      - Run clippy lints"
	@echo "  clean       - Clean build artifacts"
	@echo "  install-deps - Install required dependencies"
