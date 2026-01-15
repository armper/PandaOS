# User Mode Transition and Syscall ABI

## Overview

PandaOS implements ring-3 (user mode) execution with system call support following Linux x86_64 conventions.

## Ring Transition

### Kernel to User (Ring 0 → Ring 3)

Transition uses the `iretq` instruction:

```rust
// Set up user data segments
mov ds, USER_DS  // RPL = 3
mov es, USER_DS
mov fs, USER_DS
mov gs, USER_DS

// Push user stack frame
push USER_DS         // SS
push stack_ptr       // RSP
pushfq              // RFLAGS (with IF set)
push USER_CS        // CS (RPL = 3)
push entry_point    // RIP

// Jump to user mode
iretq
```

### User to Kernel (Ring 3 → Ring 0)

Currently via interrupts (int 0x80 style). Syscall/sysret support is planned.

## Syscall ABI

Follows Linux x86_64 calling convention:

### Registers

- **Syscall Number**: `rax`
- **Arguments** (up to 6): `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`
- **Return Value**: `rax`
- **Error**: negative errno value in `rax`

### Implemented Syscalls

#### sys_write (1)
```c
ssize_t write(int fd, const void *buf, size_t count);
```
- **fd**: File descriptor (1=stdout, 2=stderr)
- **buf**: Pointer to data
- **count**: Number of bytes to write
- **Returns**: Number of bytes written, or -errno

#### sys_exit (60)
```c
void exit(int status);
```
- **status**: Exit code
- **Returns**: Does not return

### Error Codes

POSIX errno values:
- `EBADF` (9): Bad file descriptor
- `EINVAL` (22): Invalid argument
- `ENOSYS` (38): Function not implemented

## Process Model

### Process Structure

```rust
pub struct Process {
    pub pid: Pid,
    pub state: ProcessState,
    pub entry_point: u64,
    pub user_stack_ptr: u64,
    pub kernel_stack_ptr: u64,
}
```

### Process States

- **Ready**: Process is ready to run
- **Running**: Process is currently executing
- **Exited(i32)**: Process has terminated with exit code

## ELF Loading

### Supported Format

- **Class**: ELF64 (64-bit)
- **Type**: ET_EXEC (executable)
- **Machine**: EM_X86_64
- **Endian**: Little-endian

### Validation

1. Magic number check (`0x7F 'E' 'L' 'F'`)
2. Class, endianness, version validation
3. Machine type verification
4. Program header bounds checking
5. Segment size and alignment validation

### Loading Process

1. Parse ELF header
2. Validate header fields
3. Parse program headers
4. Load PT_LOAD segments
5. Set permissions (R/W/X) per segment
6. Jump to entry point

## Memory Layout

```
0x0000_0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF  User Space
0x0000_8000_0000_0000 - 0xFFFF_7FFF_FFFF_FFFF  (Canonical Hole)
0xFFFF_8000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF  Kernel Space
```

### User Process Layout

```
0x0000_0040_0000      Text segment (.text)
0x0000_0060_0000      Data segment (.data, .bss)
0x7FFF_FFFF_F000      User stack (grows down)
```

## Safety Guarantees

1. **Type-Safe Phase Transitions**: Boot phases enforce initialization order
2. **Validated ELF Loading**: All headers defensively checked
3. **Isolated Address Spaces**: Each process has separate memory
4. **Syscall Argument Validation**: Buffer pointers and sizes checked
5. **No Globals for State**: Processes passed explicitly

## Testing

### Unit Tests

- ELF parsing (invalid magic, wrong class, etc.)
- Process state transitions
- Syscall argument validation
- Selector privilege levels

### Integration Tests

- Boot smoke test
- Frame allocator in QEMU
- PID allocator in QEMU
- Ring buffer in QEMU

## Limitations

### Current Limitations

- Single process only (no multitasking)
- No memory isolation (paging not implemented)
- No filesystem (programs loaded from memory)
- No signals
- Limited syscalls (write, exit only)
- No syscall/sysret (uses interrupts)

### Future Work

- Virtual memory and paging
- Process scheduling
- Full syscall/sysret support
- Signal handling
- Virtual file system
- Multi-core support (Phase 2)

## References

- Linux System Call ABI: https://man7.org/linux/man-pages/man2/syscall.2.html
- System V ABI (x86-64): https://refspecs.linuxbase.org/elf/x86_64-abi-0.99.pdf
- Intel SDM Volume 2: Instruction Set Reference
- Intel SDM Volume 3: System Programming Guide
