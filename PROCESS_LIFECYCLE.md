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

## exit(code)
- If process has a parent:
  - Mark process state as Zombie(code)
  - Process remains in scheduler until parent calls waitpid()
  - Page tables and resources are not freed yet
- If process has no parent (orphan):
  - Mark process state as Exited(code)
  - Queue the process for reaping
  - Page table + kernel stack frames released after CR3 switch
- Schedule the next runnable process
- If no runnable processes remain, print test marker and halt

## waitpid(pid, status_ptr, options)
- Waits for a child process to exit
- Supported options:
  - pid = -1: Wait for any child
  - pid > 0: Wait for specific child
  - options must be 0 (blocking not yet implemented)
- If zombie child found:
  - Return child PID
  - Write exit status to user memory (if status_ptr != 0)
  - Free child's page tables and kernel stack (reap)
- If no zombie children but has children:
  - Return EINTR (caller should retry)
- If no children at all:
  - Return ESRCH (no such process)

## exec(path, arg)
- Requires an absolute path and a valid ELF in the in-memory FS
- Destroys the current user address space and builds a new one
- Resets the user stack and CPU context
- Preserves the kernel stack mapping
- Preserves PID and parent_pid
- Does not return on success
- Copies arg to fixed address in new address space (0x7FFF_FFFF_C000)

## Process Reaping

After CR3 switch away from an exited process:
- Free all user-space page tables (L1, L2, L3)
- Free all user-space data pages
- Free kernel stack pages (if free_kernel_stack=true)
- Free L4 page table
- Process structure is dropped from scheduler
