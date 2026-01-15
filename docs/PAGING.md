# Virtual Memory and Paging

## Overview

PandaOS implements x86_64 4-level paging (PML4 → PDPT → PD → PT) with comprehensive safety guarantees.

## Page Table Structure

### 4-Level Paging

```
Virtual Address (48-bit):
┌─────────┬─────────┬─────────┬─────────┬──────────────┐
│ PML4 (9)│ PDPT (9)│  PD (9) │  PT (9) │  Offset (12) │
└─────────┴─────────┴─────────┴─────────┴──────────────┘
  63...39   38...30   29...21   20...12      11...0
```

Each table has 512 entries (9 bits per level).

### Page Table Entry

```rust
pub struct PageTableEntry(u64);

// Flags (bits 0-11, 63):
// - PRESENT (bit 0): Entry is valid
// - WRITABLE (bit 1): Page is writable
// - USER_ACCESSIBLE (bit 2): User mode can access
// - WRITE_THROUGH (bit 3): Write-through caching
// - NO_CACHE (bit 4): Disable caching
// - ACCESSED (bit 5): CPU has accessed this page
// - DIRTY (bit 6): Page has been written to
// - HUGE_PAGE (bit 7): 2MB or 1GB page
// - GLOBAL (bit 8): Not flushed on context switch
// - NO_EXECUTE (bit 63): Execution disabled

// Physical address: bits 12-51 (40-bit physical address)
```

## Safety Guarantees

### Compile-Time Safety

1. **Type-Safe Addresses**: `VirtAddr` and `PhysAddr` wrappers prevent mixing
2. **Aligned Structures**: `PageTable` is `#[repr(C, align(4096))]`
3. **Index Bounds**: Table indexing uses standard Rust bounds checking

### Runtime Validation

```rust
// Debug-time invariant checks
kernel_invariant!(addr.is_aligned(4096), "Page not aligned");
kernel_invariant!(entry.is_present(), "Entry not present");
```

## Page Table Management

### Creating Tables

```rust
let mut table = PageTable::new(); // Zero-initialized, 4KB aligned
table.zero(); // Clear all entries
```

### Mapping Pages

```rust
let virt = VirtAddr::new(0x1000);
let phys = PhysAddr::new(0x5000);
let flags = PageTableFlags::PRESENT
    .or(PageTableFlags::WRITABLE)
    .or(PageTableFlags::USER_ACCESSIBLE);

// Get indices
let p4_idx = virt.p4_index();
let p3_idx = virt.p3_index();
let p2_idx = virt.p2_index();
let p1_idx = virt.p1_index();

// Map at leaf level
page_table[p1_idx].set(phys.as_u64(), flags);
```

### Address Translation

```rust
let virt = VirtAddr::new(0x12345678);

// Extract components
let p4 = virt.p4_index();  // PML4 index
let p3 = virt.p3_index();  // PDPT index
let p2 = virt.p2_index();  // PD index
let p1 = virt.p1_index();  // PT index
let offset = virt.page_offset(); // Page offset

// Walk page tables to get physical address
let phys = walk_page_tables(virt)?;
```

## Memory Layout

### User Space (0x0000_0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF)

```
0x0000_0040_0000      Text segment (.text)
0x0000_0060_0000      Read-only data (.rodata)
0x0000_0080_0000      Data segment (.data, .bss)
0x7FFF_FFFF_F000      User stack (grows down)
```

### Kernel Space (0xFFFF_8000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF)

```
0xFFFF_8000_0000_0000  Physical memory direct map
0xFFFF_FFFF_8000_0000  Kernel stack
0xFFFF_FFFF_C000_0000  Kernel heap
0xFFFF_FFFF_FFFF_F000  Recursive page table mapping
```

## Page Fault Handling

### Page Fault Error Code

```
┌────┬────┬────┬────┬────┐
│ I/D│ RSVD│ U/S│ W/R│  P │
└────┴────┴────┴────┴────┘
  4    3    2    1    0

P: Present (0 = not present, 1 = protection violation)
W/R: Write/Read (0 = read, 1 = write)
U/S: User/Supervisor (0 = supervisor, 1 = user)
RSVD: Reserved bit violation
I/D: Instruction/Data (0 = data, 1 = instruction fetch)
```

### Handling Strategy

1. **Not Present**: Allocate page and update page table
2. **Protection Violation**: Check permissions, kill process if invalid
3. **Reserved Bit**: Kernel bug, panic
4. **Instruction Fetch**: Handle NX violations

## Integration with ELF Loader

### Segment Mapping

```rust
// For each PT_LOAD segment in ELF:
for segment in elf_info.load_segments {
    let virt_start = VirtAddr::new(segment.vaddr);
    let pages = (segment.mem_size + 4095) / 4096;
    
    for i in 0..pages {
        let virt = virt_start + (i * 4096);
        let phys = allocate_frame()?;
        
        let mut flags = PageTableFlags::PRESENT;
        if segment.is_writable() {
            flags = flags.or(PageTableFlags::WRITABLE);
        }
        if !segment.is_executable() {
            flags = flags.or(PageTableFlags::NO_EXECUTE);
        }
        
        map_page(virt, phys, flags)?;
    }
    
    // Copy segment data to mapped pages
    copy_segment_data(segment)?;
}
```

## Testing

### Unit Tests (7 tests)

1. **test_page_table_flags**: Flag operations (or, contains)
2. **test_page_table_entry**: Entry set/get, present/unused checks
3. **test_virt_addr_indices**: Virtual address component extraction
4. **test_page_alignment**: Address alignment operations
5. **test_page_table_size**: Size and alignment verification
6. **test_page_table_indexing**: Table entry access
7. **test_phys_addr**: Physical address operations

### Integration Tests

- Page table creation and zeroing
- Multi-level page table walk
- TLB flush operations
- User/kernel isolation

## Performance Considerations

### TLB Management

- Use GLOBAL flag for kernel pages (not flushed on context switch)
- Flush TLB selectively with `invlpg` for single pages
- Full flush with CR3 reload for process switch

### Huge Pages

- 2MB pages: Set HUGE_PAGE in PD entry, skip PT level
- 1GB pages: Set HUGE_PAGE in PDPT entry, skip PD and PT
- Reduces TLB pressure for large contiguous regions

## Security

### Protections

1. **NX Bit**: Prevent execution of writable pages
2. **User/Supervisor Isolation**: User code cannot access kernel pages
3. **SMAP/SMEP**: Supervisor cannot access user pages (when enabled)
4. **Page Table Permissions**: Page tables themselves are read-only

### Attack Mitigation

- **ASLR Ready**: Virtual address layout randomization support
- **Stack Guard Pages**: Unmapped pages detect stack overflow
- **Kernel Page Isolation**: Separate page tables for user/kernel

## Future Work

- [ ] Recursive page table mapping for easy traversal
- [ ] Copy-on-write (COW) for fork()
- [ ] Page cache for filesystem
- [ ] Swap support
- [ ] NUMA awareness
- [ ] Memory compression

## References

- Intel SDM Volume 3A: System Programming Guide, Chapter 4 (Paging)
- AMD64 Architecture Programmer's Manual Volume 2: System Programming
- Linux kernel: arch/x86/mm/
