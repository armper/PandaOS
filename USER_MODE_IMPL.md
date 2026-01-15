# User Mode Execution Implementation

## Overview
This document describes the completed implementation of user-mode execution for PandaOS, enabling the kernel to run user programs at ring 3 with full memory isolation and system call support.

## Completed Components

### 1. Paging Module Enhancements (`kernel/src/paging.rs`)

#### `create_user_page_table() -> Result<u64, &'static str>`
- Allocates a new L4 page table for user processes
- Copies kernel mappings (upper half, entries 256-511) to maintain kernel accessibility
- Returns physical address of the new page table
- Uses page table frame tracker to prevent re-allocation

#### `map_page(page_table_phys, virt_addr, phys_addr, flags) -> Result<(), &'static str>`
- Maps a virtual page to a physical frame with specified flags
- Creates intermediate page tables (L3, L2, L1) as needed
- Supports user-accessible, writable, and no-execute flags
- Used for mapping both ELF segments and user stack

#### `allocate_user_stack(page_table_phys, stack_top, num_pages) -> Result<(), &'static str>`
- Allocates and maps user stack memory
- Default: 4 pages (16KB) at 0x7FFF_FFFF_F000 (top of user space)
- Stack pages are mapped as: PRESENT | WRITABLE | USER_ACCESSIBLE | NO_EXECUTE
- Pages are zeroed for security

### 2. ELF Loader Enhancements (`kernel/src/elf.rs`)

#### `load_elf_segments(elf_info, data, page_table_phys) -> Result<(), &'static str>`
- Loads all PT_LOAD segments from ELF binary into user address space
- Allocates physical frames for each page in the segment
- Maps pages with appropriate permissions:
  - Readable segments: USER_ACCESSIBLE
  - Writable segments: +WRITABLE
  - Non-executable segments: +NO_EXECUTE
- Handles partial pages and BSS sections (zero-initialized)
- Copies segment data from ELF file to physical memory

### 3. Process Management Updates (`kernel/src/process.rs`)

#### Enhanced `Process` Structure
```rust
pub struct Process {
    pub pid: Pid,
    pub state: ProcessState,
    pub entry_point: u64,
    pub user_stack_ptr: u64,
    pub kernel_stack_ptr: u64,
    pub page_table_phys: u64,  // NEW: Page table for this process
}
```

#### `Process::new(elf_info, elf_data, pid_allocator) -> Result<Self, &'static str>`
- Creates complete process with isolated memory:
  1. Creates user page table with kernel mappings
  2. Loads ELF segments into user address space  
  3. Allocates and maps user stack
  4. Stores page table physical address
- All memory operations are performed before process runs

### 4. Syscall Handler Support (`kernel/src/syscall.rs`)

#### `set_exit_handler(handler: fn(i32) -> !)`
- Allows setting a custom exit handler for testing
- Handler is called when user process calls exit() syscall
- Used to exit QEMU with appropriate status code

### 5. User Program (`userland/hello.asm`)

Updated to use modern `syscall` instruction:
```asm
_start:
    mov rax, 1              ; syscall number for write
    mov rdi, 1              ; fd = stdout
    lea rsi, [rel message]  ; buffer
    mov rdx, message_len    ; count
    syscall                 ; Use syscall instead of int 0x80

    mov rax, 60             ; syscall number for exit
    xor rdi, rdi            ; status = 0
    syscall
```

### 6. Build System

#### `kernel/build.rs`
- Embeds userland/build/hello ELF binary into kernel at compile time
- Binary accessible as `include_bytes!(concat!(env!("OUT_DIR"), "/hello_elf"))`

#### `userland/build.sh`
- Builds hello.asm as proper ELF executable
- Uses: `nasm -f elf64 hello.asm -o hello.o`
- Links with: `ld -o hello hello.o -static -nostdlib --entry=_start`

## Memory Layout

### User Address Space (0x0000_0000_0000 - 0x0000_7FFF_FFFF_FFFF)
- ELF Code/Data: Loaded at addresses specified in ELF headers
- User Stack: 0x7FFF_FFFF_C000 - 0x7FFF_FFFF_F000 (4 pages, 16KB)

### Kernel Address Space (0xFFFF_8000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF)
- Mapped in all user page tables (upper half)
- Kernel code, data, and stacks remain accessible

## Execution Flow

1. **Process Creation**:
   - Parse ELF binary
   - Create new L4 page table
   - Copy kernel mappings to new page table
   - Map ELF segments with correct permissions
   - Allocate and map user stack
   - Initialize process structure

2. **Entering User Mode**:
   - Switch CR3 to process's page table
   - Call `enter_usermode(entry_point, stack_ptr)`
   - Use `iretq` to jump to ring 3

3. **System Call Handling**:
   - User code executes `syscall` instruction
   - CPU switches to kernel mode, jumps to `syscall_entry`
   - Handler saves user registers
   - Calls `syscall::handle_syscall()`
   - Returns to user mode with `sysretq`

## Safety Considerations

All memory operations use `unsafe` with documented safety requirements:
- Frame allocator must be initialized
- Page tables must be valid
- Physical addresses must be within valid memory range
- GDT and syscall infrastructure must be set up before entering user mode

## Current Status

### ✅ Fully Implemented
- User page table creation and management
- ELF segment loading with permissions
- User stack allocation  
- Process structure with page table support
- Syscall/sysret infrastructure (from previous PR)
- GDT with user segments (from previous PR)
- User program build system

### ⚠️ Known Limitations
1. **Physical Memory Access**: Current implementation accesses physical addresses directly, which requires identity mapping. This works for bootloader-provided identity mapping but may need refinement for production use.

2. **No TLB Flushing**: Page table switch should flush TLB for newly mapped pages.

3. **No Page Fault Handler**: Missing pages will cause kernel panic rather than proper error handling.

4. **Fixed Stack Size**: User stack is always 4 pages (16KB). Should be configurable.

5. **No Process Cleanup**: When process exits, allocated frames are not freed.

### 📋 Future Enhancements
- Implement proper higher-half kernel mapping
- Add page fault handler for demand paging
- Implement process cleanup and frame deallocation  
- Add memory-mapped file support
- Implement copy-on-write for fork()
- Add guard pages around user stack

## Testing

The implementation can be tested by:
1. Building the kernel: `cargo kbuild`
2. Creating a demo function that:
   - Calls `Process::new()` with the embedded ELF
   - Switches to the process page table
   - Enters user mode with `enter_usermode()`
3. User program should print "hello from user" and exit cleanly

Note: Full integration testing requires addressing the physical memory access limitation.

## Code Quality

- All unsafe code has SAFETY comments explaining invariants
- Functions have comprehensive documentation
- Clippy warnings addressed with targeted `#[allow]` attributes
- Follows existing safety patterns in the codebase
- Minimal changes to existing code

## Related Files

- `kernel/src/paging.rs`: Page table management
- `kernel/src/elf.rs`: ELF parsing and loading
- `kernel/src/process.rs`: Process structure and creation
- `kernel/src/syscall.rs`: System call handling
- `kernel/src/usermode.rs`: User mode transition
- `kernel/src/gdt.rs`: GDT setup (from previous PR)
- `userland/hello.asm`: Test user program
- `userland/build.sh`: User program build script
- `kernel/build.rs`: Embeds user binary in kernel

