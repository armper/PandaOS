//! Interrupt handling for x86_64
//!
//! This module sets up the Interrupt Descriptor Table (IDT) and handles
//! various CPU exceptions and hardware interrupts.

use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

/// Timer interrupt handler function pointer
static mut TIMER_HANDLER: Option<fn()> = None;

/// Initialize interrupt handling
pub fn init() {
    // SAFETY: This is called once during kernel init, we're setting up the IDT
    unsafe {
        let idt = &mut *core::ptr::addr_of_mut!(IDT);
        idt.breakpoint.set_handler_fn(breakpoint_handler);

        // Set double fault handler with separate stack to prevent triple fault
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);

        // Add page fault handler
        idt.page_fault.set_handler_fn(page_fault_handler);

        // Add timer interrupt (IRQ 0 -> interrupt 32)
        idt[32].set_handler_fn(timer_interrupt_handler);

        idt.load();
    }
}

/// Set the timer interrupt handler
///
/// This allows the scheduler to register a handler that will be called
/// on every timer tick.
///
/// # Safety
///
/// Must be called before enabling timer interrupts.
pub unsafe fn set_timer_handler(handler: fn()) {
    // SAFETY: Caller guarantees this is called before timer interrupts are enabled
    unsafe {
        TIMER_HANDLER = Some(handler);
    }
}

/// Breakpoint exception handler
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

/// Double fault exception handler
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

/// Page fault exception handler
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    // Read faulting address from CR2
    let cr2 = Cr2::read_raw();
    let rip = stack_frame.instruction_pointer.as_u64();

    // Increment page fault counter
    crate::vm_counters::inc_page_faults();

    // Parse error code bits
    let present = error_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION);
    let write = error_code.contains(PageFaultErrorCode::CAUSED_BY_WRITE);
    let user = error_code.contains(PageFaultErrorCode::USER_MODE);
    let reserved_write = error_code.contains(PageFaultErrorCode::MALFORMED_TABLE);
    let instruction_fetch = error_code.contains(PageFaultErrorCode::INSTRUCTION_FETCH);

    // Get current process from scheduler
    // SAFETY: Page fault handlers run with interrupts disabled
    let scheduler = unsafe { crate::get_scheduler() };
    let Some(process) = scheduler.current_process_mut() else {
        // No current process - kernel page fault
        panic!(
            "PAGE FAULT in kernel space: cr2={:#x}, rip={:#x}, error_code={:?}",
            cr2, rip, error_code
        );
    };

    let pid = process.pid;
    let page_table_phys = process.page_table_phys;
    let mode = if user { "user" } else { "kernel" };

    // Case 1: Not present, user mode - demand paging
    if !present && user {
        handle_demand_paging(process, cr2, rip, error_code, pid, page_table_phys, mode);
        return;
    }

    // Case 2: Present, write, user mode - potential COW fault
    if present && write && user {
        handle_cow_fault(process, cr2, rip, error_code, pid, page_table_phys, mode);
        return;
    }

    // Case 3: Not present, kernel mode accessing user space - demand paging for kernel access
    // This happens when the kernel accesses user space memory (e.g., during syscalls)
    // User space is the lower half: 0x0000_0000_0000_0000 to 0x0000_7FFF_FFFF_FFFF
    const USER_SPACE_MAX: u64 = 0x0000_8000_0000_0000;
    if !present && !user && cr2 < USER_SPACE_MAX {
        handle_demand_paging(process, cr2, rip, error_code, pid, page_table_phys, mode);
        return;
    }

    // Case 4: All other faults - protection violation or invalid access
    panic!(
        "PAGE FAULT: pid={:?}, cr2={:#x}, rip={:#x}, error_code={:?}, mode={}\n\
         present={}, write={}, user={}, reserved_write={}, instruction_fetch={}\n\
         Unhandled page fault type\n\
         Process VM regions: {:?}",
        pid,
        cr2,
        rip,
        error_code,
        mode,
        present,
        write,
        user,
        reserved_write,
        instruction_fetch,
        process.vm_regions
    );
}

/// Handle demand paging fault
fn handle_demand_paging(
    process: &mut crate::process::Process,
    cr2: u64,
    rip: u64,
    error_code: PageFaultErrorCode,
    pid: panda_hal::pid::Pid,
    page_table_phys: u64,
    mode: &str,
) {
    use crate::process::ProtFlags;

    // Check if address is in a valid VM region
    let page_addr = cr2 & !0xFFF;
    let region =
        process.vm_regions.iter().find(|r| page_addr >= r.start_addr && page_addr < r.end_addr);

    // Determine flags based on region or use defaults
    let flags = if let Some(region) = region {
        // Use region-specific flags
        let mut flags = crate::paging::PageTableFlags::PRESENT
            .or(crate::paging::PageTableFlags::USER_ACCESSIBLE);

        if region.flags & ProtFlags::PROT_WRITE.0 != 0 {
            flags = flags.or(crate::paging::PageTableFlags::WRITABLE);
        }
        if region.flags & ProtFlags::PROT_EXEC.0 == 0 {
            flags = flags.or(crate::paging::PageTableFlags::NO_EXECUTE);
        }
        flags
    } else {
        // No VM region tracking yet - use permissive defaults
        // This allows read, write, and execute for user space
        // TODO: Populate vm_regions during process creation for proper permission tracking
        crate::paging::PageTableFlags::PRESENT
            .or(crate::paging::PageTableFlags::USER_ACCESSIBLE)
            .or(crate::paging::PageTableFlags::WRITABLE)
    };

    // Allocate and map page
    // SAFETY: Frame allocator is initialized
    let frame = unsafe {
        crate::memory::allocate_frame().expect("Failed to allocate frame for demand paging")
    };

    let phys_addr = (frame as u64) * 4096;

    // Map the page
    // SAFETY: page_table_phys is valid, frame was just allocated
    unsafe {
        crate::paging::map_page(
            page_table_phys,
            crate::paging::VirtAddr::new(page_addr),
            crate::paging::PhysAddr::new(phys_addr),
            flags,
        )
        .expect("Failed to map page for demand paging");
    }

    // Zero-fill the page
    // TODO: For file-backed regions, populate from ELF data instead
    let page_virt = crate::memory::phys_to_virt(phys_addr);
    // SAFETY: page_virt points to newly allocated page
    unsafe {
        core::ptr::write_bytes(page_virt as *mut u8, 0, 4096);
    }

    // Increment demand allocation counter
    crate::vm_counters::inc_demand_allocations();

    println!(
        "PAGE FAULT: pid={:?}, cr2={:#x}, rip={:#x}, error={:?}, mode={}, action=demand_page (frame={})",
        pid, cr2, rip, error_code, mode, frame
    );
}

/// Handle copy-on-write fault
fn handle_cow_fault(
    _process: &crate::process::Process,
    cr2: u64,
    rip: u64,
    error_code: PageFaultErrorCode,
    pid: panda_hal::pid::Pid,
    page_table_phys: u64,
    mode: &str,
) {
    // Walk page table to check for COW flag
    let page_addr = cr2 & !0xFFF;

    // SAFETY: page_table_phys is valid
    let pte = unsafe {
        crate::paging::walk_page_table(page_table_phys, crate::paging::VirtAddr::new(page_addr))
    };

    let Some(pte) = pte else {
        // Page not mapped - shouldn't happen if present bit is set
        panic!(
            "PAGE FAULT: pid={:?}, cr2={:#x}, rip={:#x}, error_code={:?}, mode={}\n\
             Page table walk failed despite present bit set",
            pid, cr2, rip, error_code, mode
        );
    };

    let pte_flags = pte.flags();

    // Check if page has COW flag
    if !pte_flags.contains(crate::paging::PageTableFlags::COPY_ON_WRITE) {
        // Not a COW page - real write protection fault
        panic!(
            "PAGE FAULT: pid={:?}, cr2={:#x}, rip={:#x}, error_code={:?}, mode={}\n\
             Write to read-only page (not COW)\n\
             PTE flags: {:?}",
            pid, cr2, rip, error_code, mode, pte_flags
        );
    }

    // COW fault - allocate new frame and copy
    let old_phys = pte.addr();
    let old_frame = (old_phys / 4096) as usize;

    // Allocate new frame
    // SAFETY: Frame allocator is initialized
    let new_frame =
        unsafe { crate::memory::allocate_frame().expect("Failed to allocate frame for COW") };
    let new_phys = (new_frame as u64) * 4096;

    // Copy old page to new frame
    let old_virt = crate::memory::phys_to_virt(old_phys);
    let new_virt = crate::memory::phys_to_virt(new_phys);

    // SAFETY: Both addresses point to valid pages
    unsafe {
        core::ptr::copy_nonoverlapping(old_virt as *const u8, new_virt as *mut u8, 4096);
    }

    // Remap to new frame with RW flags, clear COW flag
    let new_flags = pte_flags.or(crate::paging::PageTableFlags::WRITABLE);

    // Clear COW flag by creating new flags without it
    let new_flags_bits = new_flags.bits() & !crate::paging::PageTableFlags::COPY_ON_WRITE.bits();
    let new_flags = crate::paging::PageTableFlags::from_bits(new_flags_bits);

    pte.set(new_phys, new_flags);

    // Flush TLB for this page
    {
        use x86_64::instructions::tlb;
        tlb::flush(x86_64::VirtAddr::new(page_addr));
    }

    // Decrement old frame refcount
    // SAFETY: Frame allocator is initialized
    unsafe {
        crate::memory::dec_frame_refcount(old_frame);
    }

    // Increment COW fault counter
    crate::vm_counters::inc_cow_faults();

    println!(
        "PAGE FAULT: pid={:?}, cr2={:#x}, rip={:#x}, error={:?}, mode={}, action=cow_copy (old_frame={}, new_frame={})",
        pid, cr2, rip, error_code, mode, old_frame, new_frame
    );
}

/// Timer interrupt handler (IRQ 0)
extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Call registered timer handler if available
    if let Some(handler) = unsafe { TIMER_HANDLER } {
        handler();
    }

    // Send EOI (End of Interrupt) to PIC
    // SAFETY: Sending EOI to PIC is safe and required after handling IRQ
    unsafe {
        crate::pic::send_eoi(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breakpoint_exception() {
        // Invoke a breakpoint exception
        x86_64::instructions::interrupts::int3();
    }
}
