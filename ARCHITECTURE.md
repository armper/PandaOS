# PandaOS Architecture

## Overview

PandaOS is a Unix-like x86_64 kernel written in Rust with a focus on clean architecture, modularity, and safety.

**SMP Status**: Single-core only until Phase 2. See [docs/SMP_STRATEGY.md](docs/SMP_STRATEGY.md) for details.

**Scheduler Status**: ✅ **COMPLETED** - Cooperative round-robin scheduler implemented (preemption planned). See [SCHEDULER.md](SCHEDULER.md) for details.

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
- **fs.rs**: In-memory VFS and per-process file descriptors (see [VFS.md](VFS.md))
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

1. **Phase 1**: Basic syscalls (exit, read/write to serial) - ✅ **COMPLETED**
2. **Phase 2**: File operations (open/read/close, read-only) - ✅ **COMPLETED**
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
- `read(fd, buf, count)` - syscall #0 (stdin from serial; file fds read-only; pipe read)
- `write(fd, buf, count)` - syscall #1 (stdout/stderr to serial; pipe write)
- `open(path, flags, mode)` - syscall #2 (read-only, absolute paths)
- `close(fd)` - syscall #3 (closes file descriptors >= 3, handles pipe refcounting)
- `pipe(pipefd)` - syscall #22 (creates pipe, returns read/write fds)
- `dup2(oldfd, newfd)` - syscall #33 (duplicates file descriptor)
- `kill(pid, sig)` - syscall #37 (sends signal to process or process group)
- `getpid()` - syscall #39 (returns process ID)
- `fork()` - syscall #57 (creates child process, returns child PID to parent, 0 to child)
- `execve(path, arg)` - syscall #59 (replaces current process image, single arg string)
- `exit(status)` - syscall #60 (terminates process)
- `waitpid(pid, status, options)` - syscall #61 (waits for child to exit, reaps zombie)
- `setpgid(pid, pgid)` - syscall #109 (sets process group ID)

## Job Control (Minimal)

**Status:** ✅ **FOUNDATIONS IMPLEMENTED** - Basic process groups and foreground tracking.

### Overview

PandaOS implements minimal job control foundations to enable correct Ctrl+C behavior for pipelines.
This provides the infrastructure for future full job control support (background jobs, Ctrl+Z, etc.).

### Process Groups (pgid)

Each process has a **process group ID** (`pgid`) that identifies which group it belongs to:
- When a process is created, it starts in its own process group (`pgid == pid`)
- Processes can join other groups using `setpgid(pid, pgid)` syscall
- Process groups enable signaling multiple related processes at once

**Use Cases:**
- Pipeline processes (e.g., `echo hi | wc`) should be in the same process group
- Allows Ctrl+C to terminate all processes in a pipeline simultaneously

### Foreground Process Group

The scheduler tracks a **foreground process group** (`foreground_pgid`):
- Only one process group can be in the foreground at a time
- The foreground group receives terminal signals (SIGINT from Ctrl+C)
- Shell sets a child's pgid as foreground before waiting for it
- Shell clears foreground pgid after child exits or receives signal

### Signal Delivery

Signals can target:
1. **Single process**: `kill(pid, signal)` where `pid > 0`
2. **Process group**: `kill(-pgid, signal)` where `pid < 0` (negative means group)

When Ctrl+C is pressed in the shell:
1. Shell checks if there's a foreground process group
2. If yes, sends SIGINT to that group: `kill(-foreground_pgid, SIGINT)`
3. All processes in that group receive SIGINT and terminate

### Implementation Details

**Process Structure** (`kernel/src/process.rs`):
```rust
pub struct Process {
    pub pid: Pid,
    pub pgid: Pid,  // Process group ID
    // ... other fields
}
```

**Scheduler API** (`kernel/src/scheduler.rs`):
```rust
impl Scheduler {
    // Track foreground process group
    pub fn set_foreground_pgid(&mut self, pgid: Option<Pid>);
    pub fn foreground_pgid(&self) -> Option<Pid>;
    
    // Signal all processes in a group
    pub fn signal_process_group(&mut self, pgid: Pid, signal: Signal) -> usize;
}
```

**Shell Behavior** (`userland/sh.asm`):
- After `fork()`, child calls `setpgid(0, 0)` to become process group leader
- Parent saves child's pgid as foreground: `foreground_pgid = child_pid`
- Parent waits for child with `waitpid(child_pid, ...)`
- On Ctrl+C, sends `kill(-foreground_pgid, SIGINT)` if foreground group exists
- After child exits, clears foreground: `foreground_pgid = 0`

### Limitations & Future Work

**Currently Supported:**
- ✅ Process groups (pgid)
- ✅ Foreground process group tracking
- ✅ SIGINT delivery to process groups
- ✅ Ctrl+C terminates foreground processes
- ✅ Works for single commands and pipelines

**Not Yet Supported (Ctrl+Z):**
- ❌ SIGTSTP (stop signal)
- ❌ SIGCONT (continue signal)
- ❌ Background jobs
- ❌ Job list management (`jobs` command)
- ❌ Foreground/background switching (`fg`, `bg`)

**Why Ctrl+Z is Not Supported:**

Implementing Ctrl+Z requires:
1. **Process state management**: Add `Stopped` state to `ProcessState` enum
2. **Signal handling**: Implement SIGTSTP and SIGCONT signal delivery
3. **Scheduler changes**: Skip stopped processes during scheduling
4. **Job tracking**: Shell must maintain a job table with stopped/background jobs
5. **TTY ownership**: Track which process group controls the terminal

This is a significant amount of complexity beyond the minimal job control needed
for correct Ctrl+C behavior. The foundations laid here (pgid + foreground tracking)
make future Ctrl+Z support straightforward when needed.

## Process Model (Post-Fork/Wait)

**VFS Model:**
- Static in-memory file table with absolute-path lookup
- Read-only byte slices for file contents
- Directory support with `getdents64` syscall
- File types: regular files and directories
- Directories: `/`, `/bin`, `/etc`
- No writes (read-only filesystem)
- Path resolution: supports relative paths, `.` and `..`

**Process State:**
- Each process has a current working directory (`cwd`)
- Initialized to `/` for new processes
- Preserved across `fork()` (child inherits parent's cwd)
- Preserved across `exec()` 
- Changed via `chdir()` syscall

**Path Resolution:**
- Absolute paths start with `/`
- Relative paths resolved against process cwd
- `.` refers to current directory
- `..` refers to parent directory
- Cannot escape root directory `/`

**FD Table:**
- Per-process fixed-size table (16 entries)
- `fd 0/1/2` are reserved for serial stdio
- `fd >= 3` are allocated on open and track per-fd offsets
- Supports four FD kinds: File, Directory, PipeRead, PipeWrite
- Directories opened for reading can be queried via `getdents64`

## Pipe Subsystem

**Design:**
- Unix-like pipes for inter-process communication
- Fixed-size 4KB ring buffer per pipe
- Non-blocking semantics with EAGAIN
- Reference-counted pipe ends

**Pipe Pool:**
- Global pool of up to 16 concurrent pipes
- Pipes allocated on demand via `pipe()` syscall
- Each pipe has independent read/write refcounts

**Pipe Semantics:**
- `pipe(pipefd)` creates two fds: read end and write end
- `write()` to write end appends data to ring buffer
- `read()` from read end consumes data from ring buffer
- When last writer closes: readers get EOF (0 bytes) when buffer empty
- When last reader closes: writers get EPIPE error
- Non-blocking: returns EAGAIN when buffer full (write) or empty (read)
- Busy-wait blocking: syscall handlers yield on EAGAIN (simple implementation)

**Fork Behavior:**
- Child inherits parent's FD table with pipe fds
- Pipe refcounts incremented for child's ends
- Parent and child can communicate via shared pipe

**FD Operations:**
- `dup2(oldfd, newfd)` duplicates pipe fds with proper refcounting
- `close(fd)` decrements pipe refcounts and triggers EOF/EPIPE

## Process Model (Post-Fork/Wait)

**Fork Semantics:**
- `fork()` creates a child process by cloning the parent
- Child receives:
  - Copy of parent's address space (full page-by-page copy)
  - Copy of parent's CPU context with rax=0 (child sees return value 0)
  - Copy of parent's FD table (independent offsets)
  - New page table with shared kernel mappings
  - New kernel stack
  - parent_pid set to parent's PID
- Parent receives child PID as return value
- Single-CPU only (no SMP support)
- No copy-on-write (COW) yet - full eager copy

**Waitpid Semantics:**
- `waitpid(pid, status, options)` waits for child to exit (blocking)
- Supported:
  - pid = -1: wait for any child
  - pid > 0: wait for specific child
  - options must be 0 (no WNOHANG)
- **Blocking behavior**: Parent process is blocked (not scheduled) until child exits
- Returns child PID when zombie found
- Writes exit status to user memory if status_ptr != 0
- Reaps child (frees page tables and kernel stack)
- Returns EINTR if woken by signal before child ready
- Returns ESRCH if no children exist
- Exit handler wakes waiting parents automatically

**Parent-Child Relationship:**
- Each process tracks parent_pid (None for init)
- On exit, process becomes zombie if parent exists
- Zombies remain in scheduler until reaped by parent
- If parent exits, orphaned children continue but reap immediately on exit

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

**Process Lifecycle:**
- `exit(code)`:
  - If has parent: becomes Zombie(code), awaits waitpid()
  - If no parent: Exited(code), queues for immediate reaping
- CR3 switches to the next runnable process (or kernel table if none)
- User-space mappings, page tables, and kernel stack frames are reclaimed after switch
- Zombie processes are never scheduled again

**exec() Semantics:**
- Replaces the current process image without changing PID or parent_pid
- Destroys user address space and builds a fresh one from ELF
- Resets user stack and CPU context
- Preserves the kernel stack mapping for the process
- Does not return on success

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
- IRQ handlers for hardware interrupts (timer implemented)
- System call interface via `syscall` instruction

## Process Scheduler

PandaOS implements a minimal cooperative round-robin scheduler. For complete documentation, see [SCHEDULER.md](SCHEDULER.md).

### Design

- **Algorithm**: Round-robin (fair time-slicing)
- **States**: Ready, Running, Exited
- **Single CPU**: No SMP support (Phase 1)
- **No priorities**: All processes equal weight

### Components

1. **Scheduler** (`kernel/src/scheduler.rs`)
   - Process queue management
   - State transitions
   - Safe Rust implementation

2. **CPU Context** (`kernel/src/context.rs`)
   - Register save/restore structure
   - 184 bytes (23 u64 fields)
   - Includes GPRs, RIP, RSP, RFLAGS, segments

3. **Context Switching** (`kernel/src/context_switch.rs`)
   - Assembly save/restore routines
   - CR3 page table switching
   - Interrupt-safe transitions

4. **Timer Infrastructure**
   - PIT driver for periodic interrupts
   - PIC driver for interrupt management
   - Timer interrupt handler (IRQ 0 → INT 32)

### Scheduling Policy

**Round-robin**:
- Processes taken from head of ready queue
- Running process moved to tail on yield/preemption
- Fair distribution of CPU time

**Preemption Points**:
- Timer interrupt (planned)
- Yield syscall (implemented, cooperative)
- Exit syscall (implemented)

### Context Switch Flow

1. **Save current process**:
   - All registers → CpuContext
   - RIP, RSP, RFLAGS, segments

2. **Select next process**:
   - Schedule_next() from scheduler
   - Round-robin selection

3. **Switch page table**:
   - Write CR3 register
   - TLB automatically flushed

4. **Restore next process**:
   - Load from CpuContext
   - Jump to saved RIP

### Interrupt/Preemption Model

**Interrupt Disable Policy**:
- Disabled during scheduler operations
- Disabled during context switches
- Disabled during page table switches
- Enabled in user mode
- Enabled in non-critical kernel code

**Critical Sections**:
- Scheduler queue manipulation
- Process state transitions
- Context switch assembly
- Page table switching

**Timer Interrupt Flow**:
```
Timer tick → IRQ 0 → PIC remap → INT 32 → IDT → timer_handler
  → (save context, schedule, restore context) → EOI → iretq
```

**Syscall Flow**:
```
User: syscall → LSTAR → syscall_entry → handler → scheduler (if needed)
  → (save context, schedule, restore context) → sysretq → User
```

**Syscall vs Interrupt Context**:
- Syscall entry saves user RIP/RFLAGS from RCX/R11 explicitly into CpuContext
- Interrupt entry gets RIP/CS/RFLAGS/RSP/SS via hardware interrupt frame
- Syscall returns with sysretq; interrupt returns with iretq
- Syscall path switches to the per-process kernel stack before calling Rust
- Syscall entry uses an arch-local pointer to the current CpuContext
- Kernel stack VA is fixed; CR3 selects per-process backing frames

### Safety Guarantees

- **No data races**: Interrupts disabled during scheduler access
- **No aliasing**: Single CPU, no concurrent access
- **Valid contexts**: All processes have initialized contexts
- **Valid page tables**: CR3 always points to valid L4 table
- **Stack safety**: RSP always points to valid memory

### Current Limitations

1. **Timer preemption not implemented**: Requires complex interrupt frame handling
2. **No sleep/wake**: Processes either ready or exited
3. **No process groups**: Basic process model only

### Future Work

- Implement timer-based preemption
- Add process sleep/wake
- Implement fork/exec syscalls

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

## Signal Support (Minimal)

PandaOS implements minimal SIGINT handling for Ctrl+C support:

### Supported Signals
- **SIGINT** (signal #2) - Interrupt signal (Ctrl+C)

### Signal Delivery
- Signals stored as bitmask in `Process.pending_signals`
- Delivered when process is scheduled via `schedule_next()`
- Default action: terminate process with exit code 130 (128 + 2)

### Syscalls
- `kill(pid, sig)` - syscall #37
  - Send SIGINT to a target process
  - Currently only works if target is current process
  - Returns ESRCH if target PID not found

### Limitations
- No custom signal handlers (only default termination)
- No other signals (SIGTSTP, SIGCONT, etc.)
- No process groups or job control
- No signal blocking or masking

### Shell Integration
- Shell detects Ctrl+C (byte 0x03) on stdin
- When idle: clears input line and reprints prompt  
- When child running: should send SIGINT to child (not yet implemented)

