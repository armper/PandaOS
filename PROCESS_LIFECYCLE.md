# Process Lifecycle

## Invariants
- Exited processes are never scheduled again.
- All user-space mappings and page tables are reclaimed on exit.
- Kernel stack frames owned by an exited process are released.
- exec() replaces the process image without changing PID.

## exit(code)
- Mark process state as Exited(code).
- Queue the process for reaping (page table + kernel stack frames).
- Schedule the next runnable process (or switch to kernel page table if none).
- Switch CR3 to the next address space before cleanup.
- Reap user mappings, page tables, and kernel stack frames after the CR3 switch.
- If no runnable processes remain, print `TEST PASS exec_smoke` (or `shell_smoke` when enabled)
  and exit QEMU after reaping.

## exec(path)
- Requires an absolute path and a valid ELF in the in-memory FS.
- Destroys the current user address space and builds a new one.
- Resets the user stack and CPU context.
- Preserves the kernel stack mapping.
- Does not return on success.
