# PandaOS Architecture

## Overview

PandaOS is a Unix-like x86_64 kernel written in Rust with a focus on clean architecture, modularity, and safety.

## Module Structure

### Kernel (`kernel/`)

The main kernel crate contains:

- **main.rs**: Kernel entry point and initialization
- **interrupts.rs**: Interrupt Descriptor Table (IDT) and exception handling
- **memory.rs**: Memory management subsystem
- Additional modules for process management, syscalls, etc. (future)

### HAL (`hal/`)

Hardware Abstraction Layer provides hardware-independent interfaces:

- **vga.rs**: VGA text mode driver for display output
- **serial.rs**: Serial port driver for debugging
- Future: PIC, APIC, timer, keyboard drivers

### Bootloader (`bootloader/`)

Currently a placeholder. Uses the `bootloader` crate for initial setup.

## Design Patterns

### 1. Hardware Abstraction Layer (HAL)

All hardware interactions go through the HAL crate. This provides:

- **Testability**: Core logic can be tested without hardware
- **Portability**: Easy to support different architectures
- **Safety**: Hardware access is isolated behind clear boundaries

### 2. Minimal Unsafe Code

Unsafe operations are:

- Isolated to HAL and specific low-level modules
- Documented with SAFETY comments
- Wrapped in safe abstractions where possible

### 3. Static Analysis

Code must pass:

- `cargo fmt` - Consistent formatting
- `cargo clippy` - Linting with zero warnings
- Unit tests - Pure logic validation
- Integration tests - QEMU-based kernel tests

## Memory Layout

```
0x0000000000000000 - 0x00007FFFFFFFFFFF: User space (future)
0xFFFF800000000000 - 0xFFFFFFFFFFFFFFFF: Kernel space
```

## Boot Process

1. Bootloader loads kernel into memory
2. Bootloader switches to long mode (64-bit)
3. Bootloader jumps to kernel `_start`
4. Kernel initializes HAL
5. Kernel sets up IDT and exception handlers
6. Kernel initializes memory management
7. Kernel enters main loop

## Interrupt Handling

- IDT (Interrupt Descriptor Table) configured on boot
- Exception handlers for CPU exceptions
- IRQ handlers for hardware interrupts (future)
- System call interface (future)

## Testing Strategy

### Unit Tests

- Run on host system
- Test pure logic without hardware dependencies
- Located in each module's `tests` submodule

### Integration Tests

- Run in QEMU
- Test actual kernel behavior
- Use custom test framework with `#![test_runner]`

## Future Plans

1. **Process Management**: Task scheduling, context switching
2. **System Calls**: POSIX-compatible syscall interface
3. **File Systems**: VFS layer with ext2/ext4 support
4. **Device Drivers**: More hardware support
5. **Networking**: TCP/IP stack
6. **User Space**: Shell, utilities
