# PandaOS Architecture

## Overview

PandaOS is a Unix-like x86_64 kernel written in Rust with a focus on clean architecture, modularity, and safety.

**SMP Status**: Single-core only until Phase 2. See [docs/SMP_STRATEGY.md](docs/SMP_STRATEGY.md) for details.

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

1. **Phase 1**: Basic syscalls (exit, write to serial) - ✅ **COMPLETED**
2. **Phase 2**: File operations (open, read, write, close)
3. **Phase 3**: Process management (fork, exec, wait)
4. **Phase 4**: Advanced features (mmap, signals, etc.)

### Syscall Mechanism

**Implementation:** Uses fast `syscall/sysret` instructions (x86_64)
- **STAR MSR**: Configures segment selectors for kernel/user transitions
- **LSTAR MSR**: Points to syscall entry point (`syscall_entry`)
- **SFMASK MSR**: Masks RFLAGS on entry (clears IF to disable interrupts)
- **EFER.SCE**: Enables syscall/sysret extensions

**Calling Convention:** Linux x86_64 ABI
- **rax**: Syscall number
- **rdi, rsi, rdx, r10, r8, r9**: Arguments (up to 6)
- **rax**: Return value (positive) or -errno (negative)
- **rcx, r11**: Preserved by hardware (store user RIP and RFLAGS)

**Implemented Syscalls:**
- `write(fd, buf, count)` - syscall #1 (stdout/stderr to serial)
- `exit(status)` - syscall #60 (terminates process)
- `getpid()` - syscall #39 (returns process ID)

## Process Model

**Structure:**
- Each process has isolated address space (separate page table)
- User stack: 16KB (4 pages) at top of user space
- Kernel stack: Separate stack for handling syscalls
- State tracking: Ready, Running, Exited(code)

**Process Creation:**
1. Parse ELF64 executable
2. Create new page table (copies kernel mappings to upper half)
3. Map PT_LOAD segments with correct permissions (R/W/X)
4. Allocate and map user stack (RW, NX, user-accessible)
5. Assign PID from allocator

**Memory Isolation:**
- Each process has its own L4 page table
- Kernel mappings (upper half) shared across all processes
- User mappings (lower half) process-specific
- Page permissions enforced by CPU (user/kernel, R/W/X, NX)

## Memory Layout

### Virtual Address Space

```
0x0000_0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF: User space
0x0000_8000_0000_0000 - 0xFFFF_7FFF_FFFF_FFFF: Canonical hole (unmapped)
0xFFFF_8000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF: Kernel space (higher-half)
```

**User Space Layout:**
- `0x0000_0040_0000`: Typical ELF program entry (text segment)
- `0x0000_0060_0000+`: Data and BSS segments
- `0x7FFF_FFFF_F000`: User stack top (grows downward, 4 pages / 16KB default)
- User pages marked with `USER_ACCESSIBLE` flag
- Non-executable stack (`NO_EXECUTE` flag set)

**Kernel Space:**
- `KERNEL_VIRT_BASE = 0xFFFF_8000_0000_0000` - Base address for kernel virtual memory
- `KERNEL_PHYS_BASE = 0x0010_0000` - Physical load address (1 MiB)
- Kernel operates in higher-half address space for security and organization
- Currently uses bootloader-provided identity mapping, transitioning to full higher-half mapping
- Future: Separate mappings with proper permissions (RX for text, R for rodata, RW+NX for data/heap)

**Physical Memory:**
- Frame size: 4 KiB
- Frame allocator: Bump allocator with explicit reservations
- Reserved regions tracked to prevent re-allocation of critical frames

**Linker Symbols for Kernel Boundaries:**
The kernel exports symbols to precisely define its memory footprint:
- `__kernel_phys_start` / `__kernel_phys_end` - Physical memory boundaries
- `__text_start` / `__text_end` - Code section
- `__rodata_start` / `__rodata_end` - Read-only data section
- `__data_start` / `__data_end` - Initialized data section
- `__bss_start` / `__bss_end` - Uninitialized data section

These symbols enable precise frame reservation instead of conservative estimates.

**Frame Reservation Strategy:**
The frame allocator implements explicit frame reservation to ensure that frames used by the kernel, bootloader, page tables, and heap are never allocated twice. This prevents memory corruption and ensures system stability.

**Reserved Frame Categories:**
1. **Frame 0 (NullFrame)**: BIOS/IVT data, never used
2. **Kernel Image**: Determined by linker symbols (kernel_phys_start to kernel_phys_end)
3. **Bootloader**: Bootloader structures including memory map and boot info
4. **Page Tables**: All page table frames (L4, L3, L2, L1) tracked and reserved
5. **Heap**: Frames allocated for kernel heap
6. **InitramfsModule**: Initial ramdisk or loaded modules (future)

**Reservation Invariants:**
- Reserved regions never overlap in the allocator's view (automatically merged)
- `allocate_frame()` always skips reserved frames
- Allocated frames ∩ reserved frames = ∅
- Once reserved, a frame remains reserved until system restart
- Page table frames are tracked in `PageTableTracker` and immediately reserved

**Page Table Tracking:**
All page table frames are tracked to ensure they're never allocated again:
- `PageTableTracker` maintains a list of all page table frames (L4, L3, L2, L1)
- When allocating frames for page tables, use `allocate_page_table_frame()`
- This immediately reserves the frame with `ReservationReason::PageTables`
- Bootloader's initial L4 page table frame is tracked during paging init
- Tests verify no overlap between allocated frames and page table frames

**Virtual Memory:**
- 4-level paging (x86_64)
- Minimal identity mapping for early boot and hardware access
- Higher-half mapping infrastructure in place (transitioning from identity mapping)
- User space demand-paged (future)

## Boot Process

1. Bootloader loads kernel into memory at physical address ~1 MiB
2. Bootloader switches to long mode (64-bit) and sets up basic paging
3. Bootloader jumps to kernel `_start`
4. Kernel initializes HAL (serial, VGA)
5. Kernel sets up IDT and exception handlers
6. Kernel initializes memory management:
   - Parses bootloader memory map
   - Initializes frame allocator with usable memory range
   - Reserves frame 0 (BIOS/IVT)
   - Reserves kernel image using linker symbols (kernel_phys_start to kernel_phys_end)
   - Reserves bootloader structures
   - Initializes paging infrastructure
7. Kernel initializes paging:
   - `paging::init_identity_map_minimal()` - Keep bootloader's identity mapping
   - `paging::init_higher_half_mapping()` - Prepare higher-half infrastructure
   - Initialize `PageTableTracker` to track all page table frames
   - Track bootloader's L4 page table frame
   - Reserve all page table frames
8. Kernel maps and initializes heap:
   - Allocates frames for heap using `allocate_frame()`
   - Immediately reserves heap frames to prevent re-allocation
   - Initializes heap allocator
9. Kernel enters main loop

**Boot Transition Invariants:**
- Identity mapping remains valid throughout boot
- Stack, GDT, and IDT pointers remain valid during paging changes
- Page table frames are tracked before any new mappings are created
- All critical structures (kernel, bootloader, page tables, heap) are reserved before allocations begin

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
