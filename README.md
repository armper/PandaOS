# PandaOS

A Unix-like x86_64 operating system kernel written in Rust, following clean modular architecture principles.

## Architecture

PandaOS is designed with strict separation of concerns:

- **kernel**: Main kernel implementation with system services
- **hal**: Hardware Abstraction Layer for x86_64 architecture
- **bootloader**: Bootloader placeholder (currently using external bootloader crate)

### Design Principles

1. **Clean Modular Architecture**: Each crate has a well-defined responsibility
2. **Strict Crate Boundaries**: Hardware-specific code is isolated in the HAL
3. **Minimal Unsafe Code**: Unsafe operations are isolated and documented
4. **HAL-based Abstraction**: Hardware interactions go through trait-based abstractions
5. **Testable Design**: Pure logic is separated from hardware-dependent code

## Building

### Prerequisites

- Rust nightly toolchain (automatically installed via `rust-toolchain.toml`)
- QEMU for testing (optional)

### Quick Start

```bash
# Install dependencies
make install-deps

# Build the kernel
make build

# Run in QEMU
make run

# Run tests
make test
```

### Build Commands

- `make build` - Build kernel in debug mode
- `make release` - Build kernel in release mode
- `make bootimage` - Create bootable disk image
- `make run` - Build and run in QEMU
- `make test` - Run all tests
- `make fmt` - Format code
- `make clippy` - Run lints

## Testing

PandaOS uses a comprehensive testing strategy:

### Host Unit Tests

Pure logic is tested on the host system:

```bash
make test-hal
```

### Kernel Integration Tests

Kernel functionality is tested in QEMU:

```bash
make test-kernel
```

## Development

### Code Style

- Follow Rust 2021 edition conventions
- Run `cargo fmt` before committing
- Ensure `cargo clippy` passes with no warnings
- Document all public APIs

### Safety

- Minimize use of `unsafe`
- Document all unsafe operations with SAFETY comments
- Use `#![deny(unsafe_op_in_unsafe_fn)]` to enforce unsafe block requirements

## Target Compatibility

PandaOS aims for POSIX/GNU compatibility where practical for a kernel implementation.

## License

GPL-3.0 (see LICENSE file)

## Contributors

PandaOS Contributors

