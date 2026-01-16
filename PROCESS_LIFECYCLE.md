# Process Lifecycle

## Invariants
- Exited processes are never scheduled again.
- All user-space mappings and page tables are reclaimed on exit or when reaped.
- Kernel stack frames owned by an exited process are released.
- exec() replaces the process image without changing PID.
- Zombie processes remain until parent calls waitpid() or parent exits.

## Process Creation

### fork()
- Creates a child process by cloning the parent
- Child gets:
  - Copy of parent's address space (full page-by-page copy, no COW yet)
  - Copy of parent's CPU context with rax=0
  - Copy of parent's FD table (shared file offsets)
  - New page table with shared kernel mappings
  - New kernel stack
  - parent_pid set to parent's PID
- Returns:
  - In parent: child PID
  - In child: 0
- Single-CPU only (no SMP support)

## Process States

- **Ready**: Process is ready to run
- **Running**: Process is currently executing
- **Zombie(code)**: Process has exited but not yet reaped by parent
- **Exited(code)**: Process has exited and has no parent (immediate reap)

## Wait States

Processes can be in one of three wait states for blocking operations:
- **NotWaiting**: Process is not blocked
- **WaitingForAnyChild**: Process is blocked waiting for any child to exit
- **WaitingForChild(pid)**: Process is blocked waiting for a specific child

Blocked processes are skipped by the scheduler until they are woken.

## exit(code)
- If process has a parent:
  - Mark process state as Zombie(code)
  - Wake any parent process waiting for this child
  - Process remains in scheduler until parent calls waitpid()
  - Page tables and resources are not freed yet
- If process has no parent (orphan):
  - Mark process state as Exited(code)
  - Queue the process for reaping
  - Page table + kernel stack frames released after CR3 switch
- Schedule the next runnable process
- If no runnable processes remain, print test marker and halt

## waitpid(pid, status_ptr, options)
- Waits for a child process to exit (blocking behavior)
- Supported options:
  - pid = -1: Wait for any child
  - pid > 0: Wait for specific child
  - options must be 0 (no WNOHANG support yet)
- If zombie child found:
  - Return child PID
  - Write exit status to user memory (if status_ptr != 0)
  - Free child's page tables and kernel stack (reap)
- If no zombie children but has children:
  - **Block the parent process** (WaitingForChild state)
  - Yield CPU to other processes
  - Wake when child exits or signal received
  - Retry finding zombie after wake
  - Return EINTR if still not ready (signal woke us)
- If no children at all:
  - Return ESRCH (no such process)

**Blocking Behavior**: Unlike the previous busy-wait implementation, waitpid now properly blocks the calling process. The scheduler will skip blocked processes, eliminating CPU spinning. When a child exits, the exit handler wakes any parent waiting for that child.

## execve(path, argv, envp) - Deep Dive

Replaces the current process image with a new ELF binary loaded from the filesystem.

### Syscall Interface

```c
long execve(const char *path, char *const argv[], char *const envp[]);
```

**Current Implementation Status:**
- ✅ Path resolution with PATH environment variable
- ✅ ELF loading and validation
- ✅ Permission checking (execute bit)
- ⚠️ **Simplified argv/envp**: Currently accepts arrays but only uses first argument
- ⚠️ **Stack layout**: Minimal implementation, not fully Linux-compatible yet
- ❌ **Full argv parsing**: Not yet implemented
- ❌ **Full envp parsing**: Not yet implemented
- ❌ **auxv support**: Minimal implementation in progress

See [ABI.md](ABI.md) for complete syscall documentation.

### Path Resolution

1. **If path contains '/'**: Treat as absolute or relative path
   - Absolute: `/mnt/bin/ls` → used directly
   - Relative: `bin/cat` → resolved against current working directory

2. **Otherwise**: Search PATH environment variable
   - Default PATH: `/mnt/bin:/bin`
   - Tries each directory in order: `dir/command`
   - First match is used
   - Returns ENOENT if not found in any PATH directory

3. **Path resolution**: Via `fs::resolve_path(cwd, path)`
   - Handles `.` and `..` components
   - Resolves to canonical absolute path

### Security Checks

Before loading, execve validates:

1. **File exists**: Returns ENOENT if path not found
2. **Is regular file**: Returns EISDIR if path is a directory
3. **Execute permission**: Returns EACCES if execute bit not set
4. **Valid ELF**: Returns ENOEXEC if file is not a valid ELF64 executable
5. **Correct architecture**: Returns ENOEXEC if not x86-64

### ELF Loading Pipeline

#### 1. File Read
Complete ELF binary loaded into memory via `fs::read_file_to_vec()`:
- Supports disk filesystem (`/mnt/bin/*`) - persistent programs
- Supports tmpfs (`/tmp/bin/*`) - temporary programs
- Falls back to in-memory filesystem if path exists there
- Returns ENOENT if file not found
- Returns EIO on read errors

#### 2. ELF Parsing
Binary validated and parsed via `elf::parse_elf()`:
- Checks ELF64 magic (`0x7F 'E' 'L' 'F'`)
- Validates class (64-bit), endianness (little), version
- Verifies machine type (x86-64) and executable type
- Extracts PT_LOAD segments with permissions
- Extracts entry point address
- Returns ENOEXEC for invalid or incompatible ELF files

#### 3. Image Replacement
Via `process::replace_image()`:
- **Create new page table**: Clones kernel mappings, fresh user space
- **Load segments**: Each PT_LOAD segment mapped with correct permissions:
  - Read-only code: PF_R | PF_X → PRESENT | USER | NO_EXECUTE = false
  - Read-only data: PF_R → PRESENT | USER | NO_EXECUTE
  - Read-write data: PF_R | PF_W → PRESENT | USER | WRITABLE | NO_EXECUTE
  - W^X enforced: No segment can be both writable and executable
- **Allocate stack**: Fresh 4-page (16KB) user stack at `0x7FFF_FFFF_F000`
- **Free old space**: Previous process address space freed after successful mapping
- Returns ENOMEM if any allocation fails

#### 4. Context Setup
CPU context reset for new entry point:
- RIP set to ELF entry point
- RSP set to top of new user stack
- All general-purpose registers zeroed
- User code segment (CS) and data segment (SS) configured
- RFLAGS set with interrupt flag enabled

#### 5. Argument Passing (Current Implementation)
**Simplified interface** - copies single string argument:
- Optional arg string copied to fixed user address (`0x7FFF_FFFF_C000`)
- NUL-terminated string accessible from user mode
- Limited to 128 bytes

**Planned Linux-Compatible Stack Layout:**
```
High Address (0x7FFFFFFFFFFF)
├─────────────────────────────
│ [argument strings]           // Actual string data
│ [environment strings]         // e.g., "PATH=/bin"
├─────────────────────────────
│ NULL                          // auxv terminator
│ AT_ENTRY, entry_point        // Auxiliary vector
│ AT_PAGESZ, 4096
│ ...
├─────────────────────────────
│ NULL                          // envp terminator
│ envp[n-1]                     // Environment pointers
│ ...
│ envp[0]
├─────────────────────────────
│ NULL                          // argv terminator  
│ argv[n-1]                     // Argument pointers
│ ...
│ argv[0]
├─────────────────────────────
│ argc                          // Argument count
└─────────────────────────────
Low Address (RSP points here)
```

See `kernel/src/exec_stack.rs` for Linux-compatible stack setup implementation (in progress).

### Exec Behavior

**Preserved across exec:**
- PID (process ID stays the same)
- Parent PID
- Process group ID (pgid)
- File descriptor table (open files remain open)
- Current working directory
- PATH environment variable

**Replaced by exec:**
- User address space (all memory mappings)
- User stack and stack pointer
- Entry point and instruction pointer
- All user-mode register values

**Error Handling:**
- Does not return on success (switches directly to new program)
- Returns error code on failure:
  - ENOENT: File not found
  - EACCES: Permission denied (no execute bit)
  - EISDIR: Path is a directory
  - ENOEXEC: Invalid or incompatible ELF file
  - ENOMEM: Out of memory during loading
  - E2BIG: Argument list too long (future)

### Supported Binary Types

✅ **Supported:**
- Static ELF64 executables (no dynamic linker)
- x86-64 architecture only
- Little-endian byte order
- ET_EXEC type (not PIE/ET_DYN)

❌ **Not Supported:**
- Dynamic linking or shared libraries
- PIE (Position Independent Executables)
- Interpreted scripts (shebangs)
- Non-x86-64 architectures
- Big-endian binaries

### Example: Exec Flow for `/mnt/bin/cat /etc/motd`

1. **Shell parses command**: Identifies program (`cat`) and args (`/etc/motd`)
2. **Shell forks**: Creates child process with same PID space
3. **Child calls execve**: `execve("/mnt/bin/cat", ["/mnt/bin/cat", "/etc/motd"], [])`
4. **Path resolution**: `/mnt/bin/cat` contains '/', used directly
5. **Permission check**: Verifies execute bit is set on `/mnt/bin/cat`
6. **Load from disk**: Reads complete ELF binary from disk filesystem
7. **Parse ELF**: Validates magic, extracts segments and entry point
8. **Replace image**: 
   - Destroys old user address space
   - Creates new page table
   - Maps code segment (RX), data segments (R or RW)
   - Allocates fresh stack
9. **Set up stack**: Copies arguments to user stack (simplified currently)
10. **Context switch**: Jumps to cat's entry point
11. **cat runs**: Opens `/etc/motd`, reads, writes to stdout, exits

### Future Enhancements

Planned improvements for full Linux compatibility:

1. **Complete argv/envp parsing**: Parse full argument and environment arrays from user space
2. **Linux-compatible stack layout**: Implement exact Linux stack format with argc, argv, envp, auxv
3. **Auxiliary vector support**: Provide AT_ENTRY, AT_PHDR, AT_PAGESZ, etc.
4. **Better error handling**: More specific errno values for edge cases
5. **execveat support**: Execute relative to directory file descriptor

## Process Reaping

After CR3 switch away from an exited process:
- Free all user-space page tables (L1, L2, L3)
- Free all user-space data pages
- Free kernel stack pages (if free_kernel_stack=true)
- Free L4 page table
- Process structure is dropped from scheduler

## Signal Handling

### Signal Types
Currently only SIGINT (signal #2) is supported:
- **SIGINT**: Interrupt signal, typically generated by Ctrl+C
- Default action: Terminate the process
- Exit code: 128 + signal number (130 for SIGINT)

### Signal Delivery

**Via kill() syscall:**
- `kill(pid, SIGINT)`: Send SIGINT to specific process
- `kill(-pgid, SIGINT)`: Send SIGINT to all processes in process group
- Signal is marked as pending in `process.pending_signals` bitmask

**Via TTY Ctrl+C:**
- User presses Ctrl+C (`0x03` byte)
- Serial input → `tty_input_byte(0x03)` → returns `TtyAction::SendSignal`
- `sys_read()` loop detects signal action
- Calls `signal_handler()` which sends SIGINT to foreground process group
- TTY buffer is cleared, `^C\n` echoed to terminal

### Signal Processing

Signals are processed during context switches in `scheduler::schedule_next()`:
1. Before re-queueing current process, check `pending_signals`
2. Call `process.deliver_signals()` to handle pending signals
3. For SIGINT: Mark process as exited with code 128 + 2 = 130
4. Process is not re-queued (removed from scheduler)

### Foreground Process Group

The scheduler tracks a foreground process group ID (`foreground_pgid`):
- Set by shell when it forks a child or creates a pipeline
- Only the foreground group receives TTY signals (Ctrl+C)
- Cleared by shell after child/pipeline completes
- Used by `signal_handler()` to target SIGINT delivery

### Signal Flow for TTY Ctrl+C

```
User presses Ctrl+C
  ↓
Serial device receives 0x03
  ↓
sys_read(0, ...) polls serial → tty_input_byte(0x03)
  ↓
TTY clears input buffer, echoes ^C\n, returns TtyAction::SendSignal
  ↓
sys_read() calls signal_handler()
  ↓
signal_handler() → scheduler.signal_process_group(foreground_pgid, SIGINT)
  ↓
All processes in foreground group have pending_signals |= SIGINT
  ↓
Next scheduler tick: deliver_signals() terminates those processes
  ↓
Shell (not in foreground group) continues, shows prompt
```

### Process Groups

Every process has a `pgid` (process group ID):
- By default, child inherits parent's pgid
- `setpgid(0, 0)` makes process a group leader (pgid = own pid)
- Pipelines: left process becomes group leader, right process joins the group
- Shell sets its children as foreground group during execution
- After child exits, shell clears foreground group

