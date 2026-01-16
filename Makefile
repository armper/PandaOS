.PHONY: all build run run-gui test clean fmt clippy install-deps fs.img

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

# Generate filesystem image
fs.img:
	@echo "Generating filesystem image..."
	python3 scripts/mkdiskimg.py
	@echo "Filesystem image created!"

# Run in QEMU (headless mode with serial output)
run: bootimage fs.img
	@echo "Starting QEMU (headless mode)..."
	cargo run --manifest-path kernel/Cargo.toml --target x86_64-unknown-none

# Run in QEMU with GUI (VGA text mode display)
run-gui: bootimage fs.img
	@echo "Starting QEMU with GUI..."
	qemu-system-x86_64 \
		-drive format=raw,file=target/x86_64-unknown-none/debug/bootimage-panda-kernel.bin \
		-drive file=fs.img,format=raw,if=ide \
		-serial stdio

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
	@echo "  fs.img      - Generate filesystem image with disk utility"
	@echo "  run         - Build and run in QEMU (headless mode with serial)"
	@echo "  run-gui     - Build and run in QEMU with VGA display window"
	@echo "  test        - Run all tests"
	@echo "  test-hal    - Run HAL tests only"
	@echo "  test-kernel - Run kernel tests only"
	@echo "  fmt         - Format code with rustfmt"
	@echo "  fmt-check   - Check code formatting"
	@echo "  clippy      - Run clippy lints"
	@echo "  clean       - Clean build artifacts"
	@echo "  install-deps - Install required dependencies"
