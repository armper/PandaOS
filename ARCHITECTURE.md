# PandaOS Architecture

## Overview

PandaOS is a Unix-like x86_64 kernel written in Rust with a focus on clean architecture, modularity, and safety.

## Design Philosophy

### Core Principles

1. **No Unsafe Outside arch_x86_64 + Drivers**: All unsafe code is restricted to architecture-specific and driver modules
2. **No Globals for Core Subsystems**: Scheduler, VFS, and VM use explicit initialization with passed references
3. **No Allocation Before Heap Init**: System panics if allocation is attempted before heap initialization
4. **Documented Invariants**: Every subsystem has documented invariants in doc comments
5. **Comprehensive Testing**: Every subsystem has host unit tests and QEMU integration tests

### Quality Gates

All code must pass:
- `cargo fmt --check` - Consistent formatting
- `cargo clippy -- -D warnings` - Zero clippy warnings
- Host unit tests - Pure logic tested on host
- QEMU integration tests - Actual kernel behavior tested

Run `./scripts/quality-gate.sh` before committing.

## Module Structure

### Kernel (`kernel/`)

The main kernel crate contains:

- **main.rs**: Kernel entry point and initialization
- **interrupts.rs**: Interrupt Descriptor Table (IDT) and exception handling
- **memory.rs**: Memory management subsystem
- Additional modules for process management, syscalls, etc. (future)

**Invariants:**
- No allocation before heap is initialized
- All subsystems initialized explicitly
- No unsafe outside arch-specific code

### HAL (`hal/`)

Hardware Abstraction Layer provides hardware-independent interfaces:

**Pure Logic Modules** (no unsafe, testable on host):
- **bitmap.rs**: Bitmap data structure for allocation tracking
- **memory.rs**: Frame allocator logic
- **pid.rs**: Process ID management
- **ringbuffer.rs**: Circular buffer implementation

**Hardware Modules** (feature-gated, documented unsafe):
- **vga.rs**: VGA text mode driver for display output
- **serial.rs**: Serial port driver for debugging

**Invariants:**
- Pure logic modules have no unsafe code
- Hardware modules document all unsafe operations with SAFETY comments
- No global mutable state in core logic

### Bootloader (`bootloader/`)

Currently a placeholder. Uses the `bootloader` crate for initial setup.

## Dependency Graph

```
kernel -> hal (pure logic + hardware)
hal -> no dependencies (except std in tests)
bootloader -> isolated
```

**Rules:**
- HAL may not depend on kernel
- Pure logic may not depend on hardware modules
- Hardware modules may only use documented unsafe

## HAL Traits

### Current Traits

None yet - hardware access is direct. Future refactoring will add:

### Planned Traits

- `FrameAllocator` - Physical memory allocation
- `PageTable` - Virtual memory management
- `InterruptController` - Interrupt handling (PIC/APIC)
- `Timer` - System timer
- `SerialPort` - Serial communication
- `BlockDevice` - Storage devices

## Syscall Strategy

### Decision: musl + Linux ABI Compatibility

**Rationale:**
- Smaller surface area than glibc
- Easier porting of existing software
- Well-documented syscall interface
- Compatible with existing tooling

**Target:**
- Linux syscall ABI (x86_64)
- POSIX-compliant where practical
- Start with minimal set: read, write, open, close, exit, fork, exec

### Syscall Implementation Plan

1. **Phase 1**: Basic syscalls (exit, write to serial)
2. **Phase 2**: File operations (open, read, write, close)
3. **Phase 3**: Process management (fork, exec, wait)
4. **Phase 4**: Advanced features (mmap, signals, etc.)

## Memory Layout

```
0x0000000000000000 - 0x00007FFFFFFFFFFF: User space (future)
0xFFFF800000000000 - 0xFFFFFFFFFFFFFFFF: Kernel space
```

**Physical Memory:**
- Frame size: 4 KiB
- Frame allocator: Bump allocator initially, bitmap later

**Virtual Memory:**
- 4-level paging (x86_64)
- Kernel identity mapped (for now)
- User space demand-paged (future)

## Boot Process

1. Bootloader loads kernel into memory
2. Bootloader switches to long mode (64-bit)
3. Bootloader jumps to kernel `_start`
4. Kernel initializes HAL (serial, VGA)
5. Kernel sets up IDT and exception handlers
6. Kernel initializes memory management (MUST happen before first allocation)
7. Kernel enters main loop

## Interrupt Handling

- IDT (Interrupt Descriptor Table) configured on boot
- Exception handlers for CPU exceptions
- IRQ handlers for hardware interrupts (future)
- System call interface via `syscall` instruction (future)

## Testing Strategy

### Host Unit Tests

- Run on host system with `cargo test --target x86_64-unknown-linux-gnu`
- Test pure logic without hardware dependencies
- Located in each module's `tests` submodule
- Current coverage: 32+ tests

### QEMU Integration Tests

- Run actual kernel in QEMU
- Use custom test framework with `#![test_runner]`
- Test output format: `TEST PASS <name>` / `TEST FAIL <name>`
- Kernel exits with code on completion (isa-debug-exit)
- Run with `./scripts/qemu-test.sh`

### Test Contract

**Every test must:**
1. Print `TEST PASS <name>` on success
2. Print `TEST FAIL <name>` on failure
3. Never hang (use timeouts)
4. Clean up resources

**QEMU runner:**
- Monitors serial output
- Parses TEST PASS/FAIL markers
- Fails on panic/timeout
- Passes on clean exit

## Anti-Footgun Measures

### Compile-Time Safety

All kernel crates use:
```rust
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::all)]
#![warn(clippy::pedantic)]
```

### Runtime Safety

- Panic on allocation before heap init
- Bounds checking on all array access
- No unwrap() in production code (use expect() with messages)

### Architecture Safety

- Unsafe code only in arch_x86_64 and driver modules
- All unsafe blocks have SAFETY comments explaining:
  - Why the operation is needed
  - What invariants make it safe
  - What could go wrong

## Definition of Done

### Per Milestone Checklist

A feature is "done" when:
- [ ] Boots in QEMU
- [ ] Logs to serial
- [ ] At least 1 QEMU integration test passes
- [ ] Unit tests for all core logic
- [ ] No new unsafe outside allowed modules (arch_x86_64, drivers)
- [ ] All unsafe blocks have SAFETY comments
- [ ] Quality gate passes (`./scripts/quality-gate.sh`)
- [ ] Documentation updated

## Future Plans

### Short Term (Current Phase)
1. **Memory Management**: Heap allocator, paging
2. **Process Management**: Task scheduling, context switching
3. **Basic I/O**: Keyboard, timer

### Medium Term
1. **System Calls**: Basic POSIX syscalls
2. **File Systems**: VFS layer with ramfs
3. **User Space**: Simple init process

### Long Term
1. **Networking**: TCP/IP stack
2. **Advanced Drivers**: More hardware support
3. **User Space Tools**: Shell, coreutils
4. **ext2/ext4 Support**: Persistent storage

## Contributing Guidelines

1. Run `./scripts/quality-gate.sh` before committing
2. Add unit tests for all pure logic
3. Add QEMU tests for kernel features
4. Document all unsafe operations
5. Update this architecture doc for major changes
6. Follow the "definition of done" checklist
