# PandaOS Scheduler Design

## Overview

PandaOS implements a minimal cooperative scheduler with round-robin scheduling. The scheduler coordinates with syscalls for voluntary yielding; timer preemption is planned but not enabled yet.

## Architecture

### Components

1. **Scheduler** (`kernel/src/scheduler.rs`)
   - Round-robin process queue
   - Process state management (Ready, Running, Exited)
   - No priorities - all processes have equal weight
   - Safe Rust implementation (no unsafe code)

2. **CPU Context** (`kernel/src/context.rs`)
   - Stores all registers needed to resume execution
   - Includes GPRs, RIP, RSP, RFLAGS, segment selectors
   - 184-byte structure (23 u64 fields)

3. **Context Switching** (`kernel/src/context_switch.rs`)
   - Assembly routines for saving/restoring context
   - CR3 switching for page table changes
   - Coordinated with scheduler for process selection

4. **Timer Infrastructure**
   - **PIT Driver** (`kernel/src/timer.rs`) - Configures timer frequency
   - **PIC Driver** (`kernel/src/pic.rs`) - Manages interrupt controller
   - **Interrupt Handler** (`kernel/src/interrupts.rs`) - Handles IRQ 0

## Invariants

### Scheduler Invariants

1. **At most one running process**: Only one process can be in Running state at any time
2. **Ready queue integrity**: Ready queue contains only Ready processes
3. **Exited processes removed**: Exited processes are removed from scheduler
4. **Schedule always succeeds**: `schedule_next()` always returns a valid process or None
5. **No data races**: Scheduler operations are atomic (interrupts disabled)

### Context Switching Invariants

1. **Interrupts disabled**: All context switches happen with interrupts disabled
2. **Valid contexts**: Processes have valid, initialized contexts before switching
3. **Valid page tables**: Page table addresses are valid and properly mapped
4. **Stack validity**: RSP always points to valid, mapped memory
5. **RIP validity**: RIP always points to valid, executable code

### Process State Transitions

```
     +-----------+
     |   Ready   |
     +-----------+
          |
          | schedule_next()
          v
     +-----------+
     |  Running  |
     +-----------+
     /    |    \
    /     |     \
   /      |      \
  v       v       v
Timer   Yield   Exit
  |       |       |
  v       v       v
Ready   Ready  Exited
```

## Implementation Details

### Global State Management

The scheduler uses a single global `SCHEDULER` static variable:

```rust
static mut SCHEDULER: Option<Scheduler> = None;
```

**Safety Guarantees:**
- Only accessed from interrupt handlers and syscalls
- Interrupts are disabled during all access
- No aliasing possible because only one CPU (SMP not supported)
- No concurrent modification because interrupts are disabled

### Scheduling Decisions

**Round-robin algorithm:**
1. Take first process from ready queue
2. Mark it as Running
3. Return reference to scheduler caller
4. On next schedule, move current to back of queue (if not exited)

### Context Switch Flow

1. **Save current context**:
   - Push all registers to CpuContext struct
   - Save RIP (return address)
   - Save RSP (stack pointer)
   - Save RFLAGS and segment selectors

2. **Switch page table**:
   - Read next process's page table address
   - Write to CR3 register (if different)
   - TLB automatically flushed

3. **Restore next context**:
   - Load all registers from CpuContext struct
   - Load new stack pointer
   - Jump to saved RIP

### Timer-Based Preemption

**Current Status: Not Implemented**

Timer preemption requires:
- Saving interrupt frame state
- Stack switching (user → kernel → new user)
- Context restoration via iretq
- Proper segment selector management

This is left for future implementation due to complexity.

### Cooperative Scheduling

**Current Status: Implemented**

Yield syscall:
- Syscall entry saves full user CPU context
- Current process moved Running → Ready
- Next process selected Ready → Running
- Context restored and sysretq returns to the new process

Exit syscall:
- Marks process as exited
- Schedules next process
- Never returns to caller

## Interrupt and Exception Handling

### Interrupt Disable Policy

**When interrupts MUST be disabled:**
- During scheduler operations
- During context switches
- During page table switches
- During critical section access

**When interrupts CAN be enabled:**
- In user mode
- In kernel code outside critical sections
- After EOI sent to PIC

### Timer Interrupt (IRQ 0)

1. CPU receives IRQ 0
2. PIC remaps to interrupt 32
3. IDT entry calls `timer_interrupt_handler`
4. Handler calls registered timer handler
5. Handler sends EOI to PIC
6. Handler returns via iretq

### Syscall Interrupts

1. User code executes `syscall` instruction
2. CPU loads RIP from LSTAR MSR
3. `syscall_entry` saves user state
4. Handler processes syscall
5. Handler returns via `sysretq`

### Syscall Context Boundaries

**Saved on syscall entry:**
- GPRs: r15..r8, rbp, rdi, rsi, rdx, rcx, rbx, rax
- User RIP from RCX → context.rip
- User RFLAGS from R11 → context.rflags
- User RSP captured before switching to kernel stack → context.rsp

**Preserved across yield():**
- All GPRs (as restored from context)
- RSP, RIP, RFLAGS
- RCX/R11 follow syscall semantics (RCX = return RIP, R11 = user RFLAGS)

**Diff vs interrupt context:**
- Syscall path saves RIP/RFLAGS manually (RCX/R11) vs interrupt frame pushes RIP/CS/RFLAGS/RSP/SS
- Syscall returns via sysretq; interrupt returns via iretq

### Kernel Stack Discipline

1. Each process has a dedicated kernel stack mapped into its page table.
2. Syscall entry switches to the current process kernel stack before calling Rust.
3. Context switches update the current syscall context pointer.
4. Kernel stack VA is fixed (`KERNEL_STACK_TOP`); CR3 selects backing frames.
5. CR3 is switched only in the sysret path that does not touch the old stack.

## Process Lifecycle

### Creation

1. Parse ELF binary
2. Create page table (copy kernel mappings)
3. Map ELF segments with correct permissions
4. Allocate user stack
5. Initialize CPU context
6. Set state to Ready
7. Add to scheduler

### Execution

1. Scheduler selects process
2. Switch to process page table
3. Restore process context
4. Jump to user mode (iretq or sysretq)

### Termination

1. Process calls exit() syscall
2. Mark process as Exited
3. Schedule next process
4. Dead process removed by scheduler

## Limitations

### Single CPU

- No SMP support
- No per-CPU scheduler
- No migration between CPUs
- Simpler locking requirements

### No Priorities

- All processes have equal weight
- No real-time scheduling
- No priority inversion problems
- Fair but not optimized for latency

### No Advanced Features

- No sleep/wake
- No process groups
- No signals (yet)
- No fork (yet)
- No exec (yet)

## Future Enhancements

### Short Term

1. Implement timer-based preemption
2. Test with multiple processes
3. Add scheduler metrics (context switch count, etc.)

### Medium Term

1. Add process sleep/wake
2. Implement fork syscall
3. Implement exec syscall
4. Add wait/waitpid for process management

### Long Term

1. SMP support (per-CPU schedulers)
2. Priority-based scheduling
3. Real-time scheduling classes
4. CPU affinity
5. Process groups and sessions
6. Signal delivery during context switch

## Testing Strategy

### Unit Tests

- Scheduler logic (add, schedule, exit)
- Process state transitions
- Context structure layout
- PIT/PIC configuration

### Integration Tests

- Single process execution
- Multi-process alternating execution
- Yield-based cooperation
- Timer-based preemption (when implemented)

### Test Programs

- `hello1.asm`: Prints message, yields 5 times, exits
- `hello2.asm`: Prints message, yields 5 times, exits
- Both use write() and yield() syscalls

## Performance Considerations

### Context Switch Cost

- Save: ~50-100 cycles (register saves + stack ops)
- CR3 switch: ~100-200 cycles (TLB flush)
- Restore: ~50-100 cycles (register loads + jump)
- **Total: ~200-400 cycles per switch**

### Scheduling Overhead

- O(1) for schedule_next() (dequeue)
- O(1) for add_process() (enqueue)
- No complex data structures
- Minimal CPU overhead

### Cache Effects

- Context switches flush TLB
- Working set must be reloaded
- Cold caches after switch
- Avoid excessive switching

## Safety and Correctness

### Memory Safety

- No unsafe in scheduler core
- All unsafe in arch-specific code
- Comprehensive SAFETY comments
- Documented invariants

### Concurrency Safety

- No data races (single CPU)
- No deadlocks (interrupts disabled)
- No priority inversion (no priorities)
- Well-defined critical sections

### Testing Coverage

- Unit tests for all scheduler operations
- Integration tests for context switching
- QEMU tests for end-to-end behavior
- Property tests for invariants

## References

- [OSDev Wiki - Scheduling](https://wiki.osdev.org/Scheduling_Algorithms)
- [Linux Kernel - CFS Scheduler](https://www.kernel.org/doc/html/latest/scheduler/sched-design-CFS.html)
- [x86_64 Calling Conventions](https://wiki.osdev.org/Calling_Conventions)
- [Intel SDM Volume 3 - System Programming Guide](https://www.intel.com/content/www/us/en/architecture-and-technology/64-ia-32-architectures-software-developer-system-programming-manual-325384.html)

---

**Last Updated**: 2026-01-15  
**Status**: Core infrastructure complete, preemption pending implementation  
**Maintainer**: PandaOS Team
