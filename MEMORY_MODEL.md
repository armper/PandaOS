# PandaOS Memory Model

This document describes the virtual memory management in PandaOS, including process address space layout, heap management via `brk`, and anonymous memory mapping via `mmap`.

## Overview

PandaOS implements a Unix-like virtual memory model with per-process address spaces. Each process has isolated virtual memory with clearly defined regions for code, data, heap, memory mappings, and stack.

## Virtual Address Space Layout

```
User Space (0x0000_0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF):
┌────────────────────────────────────────────────────────┐
│ 0x0000_0000_0000_0000                                  │
├────────────────────────────────────────────────────────┤
│ ELF .text segment (RX)                                 │  Code
│ Loaded from executable, read-only + executable         │
├────────────────────────────────────────────────────────┤
│ ELF .data segment (RW, NX)                             │  Initialized data
│ Loaded from executable, writable, no-execute           │
├────────────────────────────────────────────────────────┤
│ ELF .bss segment (RW, NX)                              │  Uninitialized data
│ Zero-initialized, writable, no-execute                 │
├────────────────────────────────────────────────────────┤
│ [heap_start] ──────────────────────────────────────┐   │
│ Heap Region (RW, NX)                               │   │  Dynamic allocation
│ Grows upward via brk()                             │   │  via brk/sbrk
│ [heap_end = current program break] ────────────────┘   │
│ ...                                                     │
│ [heap_limit = max 1GB by default] ─────────────────────│
├────────────────────────────────────────────────────────┤
│ [Gap for future expansion]                             │
├────────────────────────────────────────────────────────┤
│ 0x7FFF_0000_0000_0000                                  │
│ mmap Region (varies: RW/RX/R, based on prot flags)     │  Anonymous mappings
│ Grows downward from mmap_base                          │  via mmap()
│ Anonymous memory mappings only (no file-backed)        │
├────────────────────────────────────────────────────────┤
│ 0x7FFF_FFFF_F000                                       │
│ User Stack (RW, NX)                                    │  Stack
│ Grows downward, 4 pages (16KB) default                 │
│ 0x7FFF_FFFF_FFFF (top of user space)                   │
└────────────────────────────────────────────────────────┘

Kernel Space (0xFFFF_8000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF):
┌────────────────────────────────────────────────────────┐
│ Higher-half kernel mapping                             │
│ Kernel code, data, and global state                    │
├────────────────────────────────────────────────────────┤
│ 0xFFFF_FFFF_8000_0000                                  │
│ Per-process kernel stack (RW, NX)                      │  Syscall stack
│ 4 pages (16KB), used during syscall transitions        │
└────────────────────────────────────────────────────────┘
```

## Memory Regions

### 1. ELF Segments

**Location**: Varies by binary, typically starting at 0x400000

**Characteristics**:
- Loaded directly from ELF executable
- Permissions set per segment: RX for code, RW for data
- `.text`: Read-only, executable (code)
- `.data`: Read-write, no-execute (initialized data)
- `.bss`: Read-write, no-execute, zero-initialized (uninitialized data)

**Management**: Static, created during `exec()`, destroyed on process exit

### 2. Heap

**Location**: Immediately after ELF segments, page-aligned

**Characteristics**:
- Starts at `heap_start` (calculated as end of last ELF segment + alignment)
- Current end is `heap_end` (the program break)
- Maximum is `heap_limit` (default: heap_start + 1GB)
- Permissions: Read-write, no-execute (RW, NX)

**Management**:
- Grows via `brk(addr)` syscall (Linux syscall #12)
- Can also shrink by calling `brk()` with lower address
- Page-aligned allocations
- No overcommit: memory is allocated immediately
- No lazy allocation: pages are zeroed on allocation

**API**:
```c
// Get current program break
void *current = (void*)syscall(SYS_brk, 0);

// Set new program break (grow heap)
void *new = (void*)syscall(SYS_brk, new_addr);

// Traditional sbrk interface (implemented via brk)
void *sbrk(intptr_t increment);
```

### 3. Memory Mappings (mmap region)

**Location**: 0x7FFF_0000_0000_0000, grows downward

**Characteristics**:
- Base address at `mmap_base`, decreases with each allocation
- Anonymous mappings only (no file-backed)
- Permissions specified per mapping (PROT_READ, PROT_WRITE, PROT_EXEC)
- W^X enforced: PROT_WRITE + PROT_EXEC rejected

**Management**:
- Created via `mmap(addr, length, prot, flags, fd, offset)` syscall (Linux syscall #9)
- Currently no `munmap()` support (pages remain until process exit)
- Tracked per-process in `Process::mappings` vector

**Supported Flags**:
- `MAP_PRIVATE` (0x02): Private copy-on-write mapping (required)
- `MAP_ANONYMOUS` (0x20): Anonymous mapping, not backed by file (required)

**Unsupported (returns EINVAL)**:
- File-backed mappings (fd != -1)
- Shared mappings (MAP_SHARED)
- Fixed address mappings (MAP_FIXED)
- Any flags other than MAP_PRIVATE | MAP_ANONYMOUS

**API**:
```c
// Map 4KB of anonymous memory
void *addr = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                  MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
if (addr == MAP_FAILED) {
    // Handle error
}

// Use the memory
memset(addr, 0, 4096);
```

### 4. User Stack

**Location**: 0x7FFF_FFFF_F000 (top of user space)

**Characteristics**:
- Fixed size: 4 pages (16KB)
- Grows downward from top of user space
- Permissions: Read-write, no-execute (RW, NX)

**Management**: Static, allocated during process creation, destroyed on exit

### 5. Kernel Stack

**Location**: 0xFFFF_FFFF_8000_0000 (higher half)

**Characteristics**:
- Per-process stack in kernel space
- Used during syscall handling
- Fixed size: 4 pages (16KB)
- Permissions: Read-write, no-execute (RW, NX)

**Management**: Allocated per-process, separate from user stack

## System Calls

### brk(addr)

**Syscall Number**: 12

**Purpose**: Change the program break (heap end address)

**Arguments**:
- `addr` (u64): New program break address
  - If 0: Query current break (returns current heap_end)
  - If valid: Set new break

**Return Value**:
- On query (addr==0): Current heap_end
- On success: New heap_end (page-aligned upward)
- On failure: Current heap_end (no change)

**Error Codes**:
- ENOMEM: Out of memory (frame allocation failed)
- (No error return, just returns current break on invalid address)

**Validation**:
- Address must be >= heap_start
- Address must be <= heap_limit
- Invalid addresses return current break unchanged

**Implementation Details**:
- Page-aligns all addresses to 4KB boundaries
- Growing heap: Allocates frames and maps pages with RW|NX|USER flags
- Shrinking heap: Unmaps pages and deallocates frames
- All new pages are zeroed on allocation
- No demand paging or lazy allocation

### mmap(addr, length, prot, flags, fd, offset)

**Syscall Number**: 9

**Purpose**: Map anonymous memory into process address space

**Arguments**:
- `addr` (u64): Requested address (0 = kernel chooses)
- `length` (u64): Size in bytes
- `prot` (i32): Protection flags
  - `PROT_READ` (0x1): Read access
  - `PROT_WRITE` (0x2): Write access
  - `PROT_EXEC` (0x4): Execute access
- `flags` (i32): Mapping flags
  - `MAP_PRIVATE` (0x02): Required
  - `MAP_ANONYMOUS` (0x20): Required
- `fd` (i32): File descriptor (must be -1 for anonymous)
- `offset` (u64): File offset (ignored for anonymous)

**Return Value**:
- On success: Mapped address
- On failure: Negative errno (-EINVAL, -ENOMEM)

**Error Codes**:
- EINVAL: Invalid parameters (length==0, length>1GB, wrong flags, W+X violation, addr not page-aligned, fd!=-1)
- ENOMEM: Out of memory (frame allocation failed)

**Implementation Details**:
- Rounds length up to page size
- Enforces W^X: rejects PROT_WRITE + PROT_EXEC
- Only supports MAP_PRIVATE | MAP_ANONYMOUS
- When addr==0, allocates from mmap_base growing downward
- All new pages are zeroed on allocation
- Tracks mappings in Process::mappings vector

## Memory Safety

### W^X Enforcement

**Policy**: Writable pages cannot be executable, executable pages cannot be writable

**Implementation**:
- mmap rejects PROT_WRITE + PROT_EXEC combinations
- Heap pages: Always RW, NX
- Stack pages: Always RW, NX
- Code pages: Always RX

### Address Validation

**User Pointers**: All user-provided addresses are validated:
- Must be in user space (< 0x8000_0000_0000)
- Must not overlap kernel space
- Must be within process limits

**Page Table Isolation**: Kernel space (0xFFFF_8000_0000_0000+) is never accessible from user mode

### Memory Limits

- **Heap**: Limited to heap_limit (default 1GB)
- **mmap**: Limited by available virtual address space (grows down from 0x7FFF_0000_0000_0000)
- **Stack**: Fixed at 16KB (4 pages)

## Process Lifecycle

### Creation (fork)

1. Clone parent's page tables (user space entries 0-255)
2. Copy heap_start, heap_end, heap_limit from parent
3. Copy mappings vector from parent
4. Allocate new kernel stack
5. Child process gets copy of all heap and mmap pages

### Execution (exec)

1. Free old page tables and mappings
2. Load ELF segments from file
3. Calculate heap_start (end of last segment + page alignment)
4. Initialize heap_end = heap_start
5. Set heap_limit = heap_start + 1GB
6. Reset mmap_base = 0x7FFF_0000_0000_0000
7. Clear mappings vector
8. Allocate new user stack

### Termination (exit)

1. Free all user space pages (ELF segments, heap, mmap)
2. Free kernel stack
3. Free page table structures
4. Deallocate all frames

## Compatibility

### musl libc

PandaOS virtual memory implementation is designed for musl libc compatibility:

- `malloc()`: Uses `brk()` for small allocations, `mmap()` for large
- `free()`: Can use `brk()` to shrink heap
- Static linking: No dynamic linker, no shared libraries
- Anonymous mappings only: No file-backed mappings needed

### Linux ABI

Syscall numbers and behavior match Linux x86_64:
- `brk` (12): Compatible with Linux semantics
- `mmap` (9): Subset of Linux functionality (anonymous only)

## Limitations

### Current Implementation

- No `munmap()` support (mappings persist until process exit)
- No `mprotect()` support (cannot change permissions after mapping)
- No file-backed mappings
- No shared memory (MAP_SHARED)
- No MAP_FIXED (cannot specify exact address)
- No demand paging or lazy allocation
- No swap or overcommit
- No copy-on-write (except implicitly via fork)

### Future Enhancements

Potential improvements not in current scope:
- `munmap()` for freeing mappings
- `mprotect()` for changing page permissions
- Demand paging for heap and mmap
- File-backed mmap for memory-mapped files
- Shared memory via MAP_SHARED
- Copy-on-write optimization

## Implementation Details

### Data Structures

```rust
// Per-process heap tracking
pub struct HeapInfo {
    pub heap_start: u64,  // End of ELF data/bss
    pub heap_end: u64,    // Current program break
    pub heap_limit: u64,  // Maximum allowed (1GB default)
}

// Per-mapping tracking
pub struct MemoryMapping {
    pub addr: u64,      // Starting address
    pub length: u64,    // Size in bytes
    pub prot: u32,      // Protection flags
    pub flags: u32,     // Mapping flags
}

// Process structure additions
pub struct Process {
    // ... existing fields ...
    pub heap: HeapInfo,
    pub mappings: Vec<MemoryMapping>,
    pub mmap_base: u64,  // Current mmap allocation point
}
```

### Page Table Operations

**map_page(page_table_phys, virt_addr, phys_addr, flags)**:
- Creates 4-level page table hierarchy (L4→L3→L2→L1)
- Allocates intermediate tables as needed
- Sets page table entry with specified flags

**unmap_page(page_table_phys, virt_addr)**:
- Walks page table hierarchy
- Clears page table entry
- Returns physical address for deallocation
- Flushes TLB for unmapped address

**Flags**:
- `PRESENT`: Page is present in memory
- `WRITABLE`: Page is writable
- `USER_ACCESSIBLE`: Page is accessible from user mode
- `NO_EXECUTE`: Page is not executable (NX bit)

## Testing

### brk_smoke Test

User program that:
1. Queries current break
2. Grows heap by 8KB
3. Writes test pattern (1024 qwords)
4. Reads back and verifies
5. Shrinks heap to original size
6. Reports success

### mmap_smoke Test

User program that:
1. Maps 8KB anonymous memory
2. Writes test pattern (1024 qwords)
3. Reads back and verifies
4. Reports success

### Integration

Both tests are ELF binaries assembled from x86_64 assembly:
- Located in `userland/brk_test.asm` and `userland/mmap_test.asm`
- Built via `userland/build.sh`
- Embedded in kernel via build.rs
- Integration tests in `kernel/tests/brk_smoke.rs` and `kernel/tests/mmap_smoke.rs`

## References

- [Linux brk(2) man page](https://man7.org/linux/man-pages/man2/brk.2.html)
- [Linux mmap(2) man page](https://man7.org/linux/man-pages/man2/mmap.2.html)
- [Intel 64 and IA-32 Architectures Software Developer Manual](https://www.intel.com/content/www/us/en/architecture-and-technology/64-ia-32-architectures-software-developer-vol-3a-part-1-manual.html)
- [musl libc malloc implementation](https://git.musl-libc.org/cgit/musl/tree/src/malloc)
