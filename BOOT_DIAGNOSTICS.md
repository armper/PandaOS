# Boot Diagnostics Guide

## Overview

PandaOS includes comprehensive boot-time diagnostics to help identify and debug boot failures quickly. The system tracks boot progress through numbered steps and can provide detailed crash reports when failures occur.

## Boot Steps

The kernel logs boot progress through a series of numbered steps. Each step marker includes:
- Step number
- CPU ID (for multi-core support in future)
- CR3 register value (current page table)
- RSP register value (current stack pointer)

### Boot Step Sequence

| Step | Description |
|------|-------------|
| 1    | HAL initialized, serial output working |
| 2    | Memory subsystem initialized |
| 3    | Interrupts subsystem ready |
| 4    | Paging infrastructure established (identity map + higher-half) |
| 5    | GDT loaded |
| 6    | IDT loaded |
| 7    | Syscall/sysret MSRs configured |
| 8    | Heap region mapped |
| 9    | Heap allocator initialized |
| 10   | Mount table and filesystems ready |
| 11   | Scheduler starting (entering usermode) |

### Example Boot Log

```
BOOT STEP 1 cpu=0 cr3=0x1000 rsp=0xffff800000104f80
[BOOT] serial ok
Serial output initialized
PandaOS v0.1.0
Hardware abstraction layer initialized
BOOT STEP 2 cpu=0 cr3=0x1000 rsp=0xffff800000104f20
BOOT STEP 3 cpu=0 cr3=0x1000 rsp=0xffff800000104ee0
...
```

## Boot Assertions

Boot assertions (`BOOT_ASSERT!`) validate critical invariants during initialization. When an assertion fails, the kernel prints:

```
BOOT ASSERT FAIL code=0x<code> step=<step>
```

And then exits QEMU with failure status.

### Common Assertion Codes

| Code | Description |
|------|-------------|
| 0x100 | GDT initialization failed |
| 0x101 | IDT initialization failed |
| 0x102 | Paging setup failed |
| 0x103 | Heap mapping failed |
| 0x104 | Frame allocator exhausted |

## Panic Diagnostics

When the kernel panics, the panic handler automatically dumps boot diagnostics:

```
KERNEL PANIC: <panic message>
=== Boot Diagnostics ===
CPU: 0
CR3: 0x3000
RSP: 0xffff800000104800
Last 16 boot steps:
  [0] step 1
  [1] step 2
  [2] step 3
  [3] step 4
  [4] step 5
```

This helps identify which initialization phase was active when the panic occurred.

## Using Boot Diagnostics for Debugging

### 1. No Serial Output

If you see no serial output at all:
- Check QEMU serial configuration (`-serial file:output.log` or `-serial stdio`)
- Verify bootloader is loading correctly
- Check for early triple faults (may need hardware debugging)

### 2. Boot Stops at Specific Step

If boot stops after a specific step number:
- Look at the code between that step and the next
- Check the last boot step recorded in panic output
- Common issues:
  - Step 4: Page table corruption or invalid mapping
  - Step 8: Frame allocator out of memory
  - Step 11: Usermode transition failure

### 3. Assertion Failures

Assertion failures indicate a violated invariant:
- Check the assertion code in the output
- Review the condition that was checked
- Look at CR3/RSP values for memory corruption signs

### 4. Hang or Loop

If the kernel hangs without output:
- No boot step output: Triple fault or early bootloader issue
- Stops mid-step: Infinite loop or deadlock
- Use QEMU monitor (`-monitor stdio`) to inspect CPU state

## Boot Selfcheck Mode

The `boot-selfcheck` feature enables comprehensive validation of kernel state after initialization:

```bash
BOOT_SELFCHK=1 ./scripts/qemu-test.sh
```

This mode:
1. Initializes minimal kernel subsystems (no filesystem/scheduler)
2. Runs validation suite:
   - GDT/TSS configuration
   - IDT and interrupt handlers
   - Paging and memory management
   - Heap allocations
   - Timer configuration
3. Prints `TEST PASS boot_selfcheck` on success
4. Exits QEMU deterministically

### Reading Selfcheck Output

```
=== Boot Selfcheck Mode ===
[SELFCHECK] GDT/TSS checks...
✓ CS = 0x8 (kernel code)
✓ SS = 0x10 (kernel data)
✓ TR = 0x28 (TSS loaded)
✓ GDT/TSS checks passed

[SELFCHECK] IDT checks...
✓ IDT loaded at 0xffff800000120000, limit 0xfff
✓ LSTAR = 0xffff800000102340 (syscall entry configured)
✓ IDT checks passed

[SELFCHECK] Paging/memory checks...
✓ CR3 = 0x3000 (page table loaded)
✓ Kernel in higher-half at 0xffff800000100000
✓ Heap allocations work (allocated vec, sum=45)
✓ Paging/memory checks passed

=== Selfcheck Summary ===
✓ All checks passed
TEST PASS boot_selfcheck
```

## Integration with CI/Testing

Boot diagnostics are designed for automated testing:

1. **QEMU test harness** captures serial output to file
2. **Test markers** (`TEST PASS` / `TEST FAIL`) indicate success/failure
3. **Exit codes** allow CI to detect failures (exit code 33 = success)
4. **Deterministic** behavior ensures reproducible results

Example CI usage:

```bash
# Run boot selfcheck
BOOT_SELFCHK=1 ./scripts/qemu-test.sh

# Check for success marker
if grep -q "TEST PASS boot_selfcheck" target/qemu/boot_selfcheck.log; then
    echo "Boot selfcheck passed"
else
    echo "Boot selfcheck failed"
    cat target/qemu/boot_selfcheck.log
    exit 1
fi
```

## Best Practices

1. **Add boot steps** at major initialization boundaries
2. **Use assertions** for critical invariants that must hold
3. **Include context** in panic messages (what was being attempted)
4. **Review boot logs** when debugging to understand failure context
5. **Run selfcheck** in CI to catch regressions early
