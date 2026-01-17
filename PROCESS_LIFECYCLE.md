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
  - **Copy-on-write address space**: Physical frames shared with refcounting
  - Copy of parent's CPU context with rax=0
  - Copy of parent's FD table (shared file offsets)
  - New page table with shared kernel mappings
  - New kernel stack
  - parent_pid set to parent's PID
  - **Copy of VM regions**: heap_start, heap_end, heap_limit, mmap_base, mappings
- Returns:
  - In parent: child PID
  - In child: 0
- Single-CPU only (no SMP support)

**Copy-on-Write Implementation:**
- All user-space PTEs marked read-only + COW flag in both parent and child
- Physical frames shared between parent and child
- Refcounts incremented for each shared frame
- Write faults trigger copy-on-write: allocate new frame, copy data, remap writable
- Read-only pages remain shared until written to
- Parent and child eventually get independent physical frames only for modified pages

**Benefits:**
- Faster fork (no immediate copying)
- Reduced memory usage (share unmodified pages)
- Deferred allocation (copy frames only when modified)

## Process States

- **Ready**: Process is ready to run
- **Running**: Process is currently executing
- **Stopped**: Process is suspended (stopped by SIGTSTP), not scheduled until SIGCONT
- **Zombie(code)**: Process has exited but not yet reaped by parent
- **Exited(code)**: Process has exited and has no parent (immediate reap)

## Wait States

Processes can be in one of three wait states for blocking operations:
- **NotWaiting**: Process is not blocked
- **WaitingForAnyChild**: Process is blocked waiting for any child to exit or stop
- **WaitingForChild(pid)**: Process is blocked waiting for a specific child to exit or stop

Blocked processes are skipped by the scheduler until they are woken.

## Signal-Induced State Transitions

### SIGTSTP (Stop Signal)
When a process receives SIGTSTP (typically from Ctrl+Z):
1. Process transitions to **Stopped** state
2. Parent waiting with WUNTRACED is woken
3. Process is skipped by scheduler until resumed
4. Resources (memory, file descriptors) remain allocated

### SIGCONT (Continue Signal)
When a stopped process receives SIGCONT:
1. Process transitions from **Stopped** to **Ready**
2. Process becomes schedulable again
3. Execution resumes from where it was stopped

### State Diagram
```
Ready ←→ Running → Stopped (SIGTSTP)
  ↑                    ↓
  └──────(SIGCONT)─────┘
  
Running → Exited/Zombie (exit or SIGINT)
```

## exit(code)
- If process has a parent:
  - Mark process state as Zombie(code)
  - Wake any parent process waiting for this child
  - Process remains in scheduler until parent calls waitpid()
  - Page tables and resources are not freed yet
  - **Heap and mmap regions remain until reaped**
- If process has no parent (orphan):
  - Mark process state as Exited(code)
  - Queue the process for reaping
  - Page table + kernel stack frames released after CR3 switch
  - **All heap and mmap pages deallocated**
- Schedule the next runnable process
- If no runnable processes remain, print test marker and halt

## waitpid(pid, status_ptr, options)
- Waits for a child process to change state (exit or stop)
- Supported options:
  - pid = -1: Wait for any child
  - pid > 0: Wait for specific child
  - **WUNTRACED** (0x2): Also report stopped children
  - options = 0: Only wait for exited children (default)
  
- **If stopped child found** (with WUNTRACED):
  - Return child PID
  - Write stop status to user memory: `(signal << 8) | 0x7f`
  - **Do NOT reap** - stopped process remains in scheduler
  - Example: SIGTSTP (20) → status = 0x147f
  
- If zombie child found:
  - Return child PID
  - Write exit status to user memory (if status_ptr != 0): `exit_code << 8`
  - Free child's page tables and kernel stack (reap)
  - **Deallocate all heap and mmap pages**
  
- If no zombie or stopped children but has children:
  - **Block the parent process** (WaitingForChild state)
  - Yield CPU to other processes
  - Wake when child exits, stops (with WUNTRACED), or signal received
  - Retry finding zombie or stopped child after wake
  - Return EINTR if still not ready (signal woke us)
  
- If no children at all:
  - Return ESRCH (no such process)

**Status Encoding:**
- **Exited**: `(exit_code << 8)` - e.g., exit(0) → 0x0000, exit(1) → 0x0100
- **Stopped**: `(signal << 8) | 0x7f` - e.g., SIGTSTP → 0x147f
- **Signaled**: `128 + signal` - e.g., SIGINT → 130

Shell can use macros to decode:
- `WIFEXITED(status)`: `(status & 0x7f) == 0`
- `WIFSTOPPED(status)`: `(status & 0xff) == 0x7f`
- `WEXITSTATUS(status)`: `(status >> 8) & 0xff`
- `WSTOPSIG(status)`: `(status >> 8) & 0xff`

**Blocking Behavior**: Unlike the previous busy-wait implementation, waitpid now properly blocks the calling process. The scheduler will skip blocked processes, eliminating CPU spinning. When a child exits or stops, the scheduler wakes any parent waiting for that child.

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
- **Reset heap**: New heap_start calculated after ELF segments, heap_end = heap_start
- **Clear mmap**: All mmap mappings cleared, mmap_base reset to default
- **Free old space**: Previous process address space freed after successful mapping
- Returns ENOMEM if any allocation fails

**Key Invariants:**
- exec does NOT preserve heap allocations
- exec does NOT preserve mmap regions
- exec resets process memory to clean slate
- Only PID, parent_pid, and file descriptors are preserved

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

## Dynamic Memory Management (brk/mmap)

### brk Syscall (#12)

**Purpose**: Manage process heap allocation

**Interface**: `brk(addr: u64) -> u64`
- If `addr == 0`: Return current heap end (program break)
- If `addr != 0`: Set new heap end (with validation)
- Returns new heap end on success, or current heap end on failure

**Behavior**:
1. **Query current break**: `brk(0)` returns `heap_end` without modification
2. **Grow heap**: If `addr > heap_end`:
   - Validates `addr <= heap_limit` (1GB max by default)
   - Checks for collision with mmap region (`addr <= mmap_base`)
   - Allocates and maps new pages with RW+NX+USER flags
   - Zeros all newly mapped pages
   - Updates `heap_end` to new address
   - Returns new `heap_end`
3. **Shrink heap**: If `addr < heap_end`:
   - Unmaps whole pages between new and old break
   - Deallocates physical frames
   - Updates `heap_end` to new address
   - Returns new `heap_end`

**Error Conditions**:
- Returns `ENOMEM` if would collide with mmap region
- Returns current `heap_end` (not error) if addr out of bounds

**Fork Behavior**:
- Child inherits parent's heap state (heap_start, heap_end, heap_limit)
- Child gets independent physical frames (eager copy)
- Child can grow/shrink heap independently

**Exec Behavior**:
- Heap state completely reset
- New heap_start calculated from new ELF segments
- heap_end = heap_start (no allocated heap)

### mmap Syscall (#9)

**Purpose**: Allocate anonymous memory regions

**Interface**: `mmap(addr, length, prot, flags, fd, offset) -> u64`
- Only supports `MAP_PRIVATE | MAP_ANONYMOUS`
- `fd` must be -1 for anonymous mappings
- Returns mapped address on success, negative errno on failure

**Supported Flags**:
- `MAP_PRIVATE` (0x02): Required
- `MAP_ANONYMOUS` (0x20): Required
- All other flags: Rejected with `EINVAL`

**Protection Flags**:
- `PROT_READ` (0x1): Read access
- `PROT_WRITE` (0x2): Write access
- `PROT_EXEC` (0x4): Execute access
- `PROT_WRITE | PROT_EXEC`: Rejected with `EINVAL` (W^X enforcement)

**Behavior**:
1. **Validate parameters**:
   - length > 0 and <= 1GB
   - flags = MAP_PRIVATE | MAP_ANONYMOUS
   - fd = -1
   - W^X: not (PROT_WRITE && PROT_EXEC)
2. **Choose address**:
   - If addr == 0: Allocate from mmap_base downward
   - If addr != 0: Use specified address (must be page-aligned)
3. **Check collision**:
   - Validates addr < KERNEL_SPACE_START
   - Checks addr >= heap_end (no overlap with heap)
4. **Allocate pages**:
   - Rounds length up to page boundary
   - Allocates physical frames
   - Maps pages with specified protection + USER flag
   - Zeros all pages
5. **Track mapping**:
   - Adds entry to process `mappings` Vec
   - Stores addr, length, prot, flags

**Error Conditions**:
- `EINVAL`: Bad flags, W^X violation, misaligned address
- `ENOMEM`: Would collide with heap, or allocation failed

**Fork Behavior**:
- Child inherits parent's mmap_base
- Child gets copy of all mappings Vec entries
- Child gets independent physical frames (eager copy)
- Child can mmap independently without affecting parent

**Exec Behavior**:
- All mappings cleared
- mmap_base reset to default (0x7FFF_0000_0000_0000)
- No mmap regions from old process retained

### Memory Collision Prevention

The kernel enforces strict separation between heap and mmap regions:

```
Low Address
┌─────────────────────────┐
│ ELF Segments            │ (Code, Data, BSS)
├─────────────────────────┤
│ ↓ Heap (grows down)     │ brk grows upward
│                         │
│     (guard region)      │ <- Collision detection here
│                         │
│ ↑ mmap (grows up)       │ mmap grows downward
├─────────────────────────┤
│ User Stack              │ (Fixed at 0x7FFF_FFFF_F000)
└─────────────────────────┘
High Address
```

**Invariants**:
- `heap_end <= mmap_base` (always enforced)
- brk validates `new_addr <= mmap_base` before growing
- mmap validates `addr >= heap_end` before mapping
- Both return ENOMEM on collision

### Usage Examples

**Simple heap allocation**:
```c
// Get current break
char *heap_start = (char *)brk(0);

// Grow heap by 8KB
char *new_break = (char *)brk((uint64_t)heap_start + 8192);

// Use heap
memset(heap_start, 0, 8192);

// Shrink back
brk((uint64_t)heap_start);
```

**Anonymous mmap**:
```c
// Map 16KB read-write region
void *addr = mmap(NULL, 16384, PROT_READ|PROT_WRITE,
                  MAP_PRIVATE|MAP_ANONYMOUS, -1, 0);
if ((long)addr < 0) {
    // Error
}

// Use memory
memset(addr, 0, 16384);

// No munmap yet - stays until process exits
```

### Integration with musl malloc

musl libc's malloc uses both brk and mmap:
- **Small allocations** (<128KB): Use brk to grow heap
- **Large allocations** (>=128KB): Use mmap for dedicated regions
- This hybrid strategy is fully supported by PandaOS
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
- **Decrement refcounts for all user-space data pages**
- Frames automatically freed when refcount reaches 0
- Free kernel stack pages (if free_kernel_stack=true)
- Free L4 page table
- Process structure is dropped from scheduler

**Refcounting Cleanup:**
- Each page table entry decrement's frame refcount on unmap
- Shared frames (from COW fork) remain until all processes release them
- No double-free: frames only deallocated when refcount == 0

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

