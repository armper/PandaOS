# PandaOS System Call ABI Documentation

## Overview

PandaOS implements a Linux-compatible x86_64 syscall ABI to enable running standard userspace programs compiled for Linux. This document describes the exact ABI contract and known deviations.

## Syscall Calling Convention

PandaOS follows the standard Linux x86_64 syscall ABI:

### Register Usage

| Register | Purpose | Preserved? |
|----------|---------|-----------|
| `rax` | Syscall number (input), Return value (output) | Modified |
| `rdi` | Argument 1 | Preserved |
| `rsi` | Argument 2 | Preserved |
| `rdx` | Argument 3 | Preserved |
| `r10` | Argument 4 | Preserved |
| `r8` | Argument 5 | Preserved |
| `r9` | Argument 6 | Preserved |
| `rcx` | Return address (clobbered by syscall instruction) | Clobbered |
| `r11` | RFLAGS (clobbered by syscall instruction) | Clobbered |

**Note:** All other general-purpose registers are preserved across syscalls.

### Syscall Instruction

- **Entry:** `syscall` instruction
- **Exit:** `sysretq` instruction
- **Kernel Entry Point:** Configured via MSR_LSTAR
- **Segment Selectors:** Configured via MSR_STAR

### Return Value Convention

- **Success:** Non-negative value in `rax` (0 or positive result)
- **Error:** Negative errno value in `rax` (e.g., `-ENOENT` = -2)
- **Never:** Sets a separate errno variable (unlike libc wrappers)

Example:
```
Success: rax = 3 (file descriptor)
Error:   rax = -2 (ENOENT - file not found)
```

## Implemented Syscalls

| Number | Name | Arguments | Return | Status |
|--------|------|-----------|--------|--------|
| 0 | read | fd, buf, count | bytes_read or -errno | ✅ Implemented |
| 1 | write | fd, buf, count | bytes_written or -errno | ✅ Implemented |
| 2 | open | path, flags, mode | fd or -errno | ✅ Implemented |
| 3 | close | fd | 0 or -errno | ✅ Implemented |
| 4 | stat | path, statbuf | 0 or -errno | ✅ Implemented |
| 5 | fstat | fd, statbuf | 0 or -errno | ✅ Implemented |
| 22 | pipe | pipefd[2] | 0 or -errno | ✅ Implemented |
| 33 | dup2 | oldfd, newfd | newfd or -errno | ✅ Implemented |
| 37 | kill | pid, sig | 0 or -errno | ✅ Implemented |
| 39 | getpid | - | pid | ✅ Implemented |
| 57 | fork | - | child_pid or 0 or -errno | ✅ Implemented |
| 59 | execve | path, argv, envp | no return or -errno | ⚠️ Partial |
| 60 | exit | status | never returns | ✅ Implemented |
| 61 | wait4 | pid, status, options, rusage | pid or -errno | ✅ Implemented |
| 79 | getcwd | buf, size | 0 or -errno | ✅ Implemented |
| 80 | chdir | path | 0 or -errno | ✅ Implemented |
| 87 | unlink | path | 0 or -errno | ✅ Implemented |
| 90 | chmod | path, mode | 0 or -errno | ✅ Implemented |
| 109 | setpgid | pid, pgid | 0 or -errno | ✅ Implemented |
| 217 | getdents64 | fd, dirp, count | bytes_read or -errno | ✅ Implemented |

### Custom Syscalls (Non-Linux)

| Number | Name | Arguments | Return | Notes |
|--------|------|-----------|--------|-------|
| 63 | getenv | name, buf, size | 0 or -errno | PandaOS-specific |

## Error Codes (errno)

PandaOS implements standard POSIX error codes:

| Code | Value | Name | Description |
|------|-------|------|-------------|
| EPERM | 1 | Operation not permitted | |
| ENOENT | 2 | No such file or directory | |
| ESRCH | 3 | No such process | |
| EINTR | 4 | Interrupted system call | |
| EIO | 5 | I/O error | |
| E2BIG | 7 | Argument list too long | |
| ENOEXEC | 8 | Exec format error | |
| EBADF | 9 | Bad file descriptor | |
| EAGAIN | 11 | Try again | |
| ENOMEM | 12 | Out of memory | |
| EACCES | 13 | Permission denied | |
| EFAULT | 14 | Bad address | |
| EEXIST | 17 | File exists | |
| ENOTDIR | 20 | Not a directory | |
| EISDIR | 21 | Is a directory | |
| EINVAL | 22 | Invalid argument | |
| EMFILE | 24 | Too many open files | |
| EROFS | 30 | Read-only filesystem | |
| EPIPE | 32 | Broken pipe | |
| ERANGE | 34 | Result too large | |
| ENOSYS | 38 | Function not implemented | |
| ENOTEMPTY | 39 | Directory not empty | |

## Process Execution (execve)

### Current Implementation

PandaOS currently implements a simplified execve interface:

```c
// Current simplified interface (subject to change)
long execve(const char *path, const char *single_arg, NULL);
```

- ✅ Loads static ELF64 executables
- ✅ Replaces process image in-place (preserves PID)
- ✅ Sets up user stack
- ⚠️ **Limited:** Only supports a single string argument
- ⚠️ **No argv array support yet**
- ⚠️ **No envp array support yet**
- ⚠️ **No auxv support yet**

### Planned Linux-Compatible Stack Layout

The following stack layout is planned for full Linux compatibility:

```
High Address (0x7FFFFFFFFFFF)
├─────────────────────────────
│ [envp strings]
│ [argv strings]
│ [executable path]
├─────────────────────────────
│ NULL (auxv terminator)
│ AT_ENTRY, entry_point
│ AT_PHDR, program_headers
│ ... (more auxv pairs)
├─────────────────────────────
│ NULL (envp terminator)
│ envp[n-1]
│ ...
│ envp[0]
├─────────────────────────────
│ NULL (argv terminator)
│ argv[n-1]
│ ...
│ argv[0]
├─────────────────────────────
│ argc
└─────────────────────────────
Low Address (stack pointer)
```

**Status:** ⚠️ Minimal implementation in progress

## Known Deviations from Linux

### Major Limitations

1. **No Dynamic Linking**
   - Only static ELF64 executables are supported
   - No shared libraries (.so files)
   - No dynamic linker (/lib64/ld-linux-x86-64.so.2)

2. **No Memory Management Syscalls**
   - mmap/munmap not implemented
   - mprotect not implemented
   - brk/sbrk not implemented
   - Process memory is fixed at exec time

3. **Limited Signal Support**
   - Only SIGINT (Ctrl+C) is implemented
   - No signal handlers (sigaction)
   - No signal masking
   - Signals terminate the process with exit code 128 + signum

4. **No Threading**
   - clone() not implemented
   - No POSIX threads
   - Single-threaded processes only

5. **Simplified execve**
   - Limited argument passing (single string, not argv array)
   - No environment variable array (envp)
   - Minimal auxv support

6. **No Users/Groups**
   - No uid/gid/euid/egid
   - No permission checks
   - All processes run with full privileges

7. **Limited File Operations**
   - No fcntl
   - No ioctl
   - No file locking
   - Basic O_RDONLY, O_WRONLY, O_RDWR, O_CREAT, O_TRUNC only

### Minor Deviations

1. **File Descriptors**
   - Limited FD table size (64 per process)
   - No fd passing between processes

2. **Process Groups**
   - Basic pgid support for job control
   - No full session management

3. **Directory Operations**
   - getdents64 returns simplified directory entries
   - No d_type field support yet

## Compatibility Level

### What Works

✅ **Basic Programs:**
- Simple "hello world" programs
- Programs that read/write stdio
- Programs that fork and exec
- Shell scripts (via custom shell)

✅ **Static Binaries:**
- Programs compiled with `-static` flag
- musl libc static binaries (with limitations)
- Hand-coded assembly programs

✅ **Process Management:**
- fork() to create child processes
- execve() to load new programs
- wait4() to reap children
- exit() with status codes

### What Doesn't Work

❌ **Dynamic Binaries:**
- Shared libraries
- Position-independent executables (PIE)
- Dynamic linker

❌ **Advanced Features:**
- Multi-threading
- Signal handlers
- Memory mapping
- Network sockets
- Advanced file operations

❌ **Complex Programs:**
- Most GNU coreutils (due to dynamic linking)
- Programs requiring mmap
- Programs requiring signals beyond SIGINT
- Programs requiring threads

## Testing Compatibility

To test if a program will run on PandaOS:

1. **Check if static:**
   ```bash
   file program
   # Should say: "statically linked"
   ```

2. **Check architecture:**
   ```bash
   file program
   # Should say: "x86-64"
   ```

3. **Check syscalls:**
   ```bash
   strace program 2>&1 | grep syscall_name
   # Verify all syscalls are in the implemented list above
   ```

## Future Improvements

Planned enhancements to reach better Linux compatibility:

1. **Phase 1: Complete execve** (In Progress)
   - Full argv array support
   - Full envp array support
   - Minimal auxv support

2. **Phase 2: Memory Management**
   - Implement mmap/munmap
   - Anonymous memory mapping
   - File-backed mappings

3. **Phase 3: Extended Signals**
   - Signal handlers (sigaction)
   - Signal delivery
   - Signal masking

4. **Phase 4: Advanced I/O**
   - fcntl
   - poll/select
   - Non-blocking I/O

## Example: Building Compatible Programs

### Using musl-gcc

```bash
# Install musl cross-compiler
# On Ubuntu/Debian:
sudo apt-get install musl-tools

# Compile static binary
x86_64-linux-musl-gcc -static -o hello hello.c

# Verify it's static
file hello
# hello: ELF 64-bit LSB executable, x86-64, version 1 (SYSV), statically linked

# Copy to PandaOS disk image
cp hello /path/to/pandaos/userland/bin/
```

### Minimal C Program

```c
// hello.c - Compatible with PandaOS
#include <unistd.h>

int main() {
    const char *msg = "Hello from musl!\n";
    write(1, msg, 17);
    return 0;
}
```

## Version History

- **v0.1** (Current): Basic syscall interface, simplified execve
- **v0.2** (Planned): Full execve with argv/envp, improved errno handling
- **v0.3** (Future): Memory management syscalls (mmap, brk)

## References

- [Linux x86_64 Syscall ABI](https://github.com/torvalds/linux/blob/master/arch/x86/entry/calling.h)
- [Linux Syscall Reference](https://man7.org/linux/man-pages/man2/syscalls.2.html)
- [System V ABI x86-64](https://refspecs.linuxbase.org/elf/x86_64-abi-0.99.pdf)
- [musl libc](https://musl.libc.org/)
