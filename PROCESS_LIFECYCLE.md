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

## exec(path, arg)

Replaces the current process image with a new ELF binary loaded from the filesystem.

### Path Resolution
- If path contains '/', treat as absolute or relative path
- Otherwise, search PATH environment variable (e.g., `/mnt/bin:/bin`)
- Resolved via `fs::resolve_path()` against current working directory

### ELF Loading Pipeline
1. **File Read**: Complete ELF binary loaded into memory via `fs::read_file_to_vec()`
   - Supports disk filesystem (`/mnt/bin/*`)
   - Supports tmpfs (`/tmp/bin/*`)
   - Falls back to in-memory filesystem if path exists there
2. **ELF Parsing**: Binary validated and parsed via `elf::parse_elf()`
   - Checks ELF64 magic, class, endianness
   - Validates x86-64 static executable
   - Extracts PT_LOAD segments and entry point
3. **Image Replacement**: Via `process::replace_image()`
   - Creates new user page table
   - Maps PT_LOAD segments with correct permissions (W^X enforced)
   - Allocates fresh 4-page user stack at `0x7FFF_FFFF_F000`
   - Frees old address space after successful mapping
4. **Context Setup**: CPU context reset for new entry point
5. **Argument Passing**: Optional arg string copied to fixed user address (`0x7FFF_FFFF_C000`)

### Exec Behavior
- Destroys the current user address space and builds a new one
- Resets the user stack and CPU context
- Preserves the kernel stack mapping
- Preserves PID, parent_pid, file descriptor table, and working directory
- Does not return on success (switches directly to new program)
- Returns error on failure (e.g., file not found, invalid ELF, out of memory)

### Supported Binary Types
- Static ELF64 executables only
- No dynamic linking or shared libraries
- No PIE support

## Process Reaping

After CR3 switch away from an exited process:
- Free all user-space page tables (L1, L2, L3)
- Free all user-space data pages
- Free kernel stack pages (if free_kernel_stack=true)
- Free L4 page table
- Process structure is dropped from scheduler
