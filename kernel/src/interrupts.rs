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

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("{:#?}", stack_frame);

    // For now, halt on page fault
    loop {
        x86_64::instructions::hlt();
    }
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
