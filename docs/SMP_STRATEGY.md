# SMP (Symmetric Multiprocessing) Strategy

## Decision: Single-Core Only Until Phase 2

**Status**: Single-core only  
**Target**: Multi-core support in Phase 2 (after scheduler and memory management are stable)

## Rationale

PandaOS will initially target **single-core execution only** for the following reasons:

1. **Simplicity First**: Single-core allows us to establish correct algorithms without concurrent access complexity
2. **Incremental Complexity**: We can add SMP after core subsystems are proven correct
3. **Clear Migration Path**: Lock-free data structures and atomic operations are used from day one where appropriate (e.g., PID allocator)
4. **Known Upgrade Point**: When we add SMP, we know exactly what needs review

## Current Implementation

### Single-Core Assumptions

The following subsystems currently assume single-core execution:

- **Frame Allocator** (`hal/src/memory.rs`): Simple bump allocator, no locking
- **VGA Driver** (`hal/src/vga.rs`): Uses spinlock but assumes single-core
- **Serial Driver** (`hal/src/serial.rs`): Uses spinlock but assumes single-core
- **Interrupt Handling** (`kernel/src/interrupts.rs`): IDT setup assumes BSP only

### Already SMP-Safe Components

These components are designed to be SMP-safe from the start:

- **PID Allocator** (`hal/src/pid.rs`): Uses `AtomicU64` for allocation
- **Ring Buffer** (`hal/src/ringbuffer.rs`): Can be made lock-free with minor changes
- **Bitmap** (`hal/src/bitmap.rs`): Thread-safe with external synchronization

## Phase 2: SMP Support

When we add multi-core support, the following changes will be required:

### 1. AP (Application Processor) Bootstrap
- Boot secondary CPUs via APIC
- Initialize per-CPU data structures
- Set up per-CPU stacks and GDT/IDT

### 2. CPU-Local Storage
```rust
// Future: Per-CPU data
struct CpuLocal {
    id: u32,
    current_task: Option<TaskId>,
    local_allocator: FrameAllocator,
}
```

### 3. Lock Improvements
- Ticket locks (fair ordering)
- Per-CPU lock tracking
- Lock-free structures where possible

## Migration Checklist

When adding SMP support, verify:
- [ ] All shared mutable state is protected
- [ ] Lock ordering is documented
- [ ] Per-CPU data uses proper accessors
- [ ] TLB invalidation is broadcast
- [ ] Atomic operations use correct ordering

---

**Summary**: We are explicitly single-core only until Phase 2. This is a deliberate choice to reduce complexity while establishing correct fundamentals.
