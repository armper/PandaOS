# Process Lifecycle

## Invariants
- Exited processes are never scheduled again.
- All user-space mappings and page tables are reclaimed on exit.
- Kernel stack frames owned by an exited process are released.
- exec() replaces the process image without changing PID.

## exit(code)
- Mark process state as Exited(code).
- Reclaim user mappings and page tables.
- Release kernel stack frames.
- Remove the process from the scheduler run queue.
- If no runnable processes remain, print `TEST PASS exec_smoke` and exit QEMU.

## exec(path)
- Requires an absolute path and a valid ELF in the in-memory FS.
- Destroys the current user address space and builds a new one.
- Resets the user stack and CPU context.
- Preserves the kernel stack mapping.
- Does not return on success.
