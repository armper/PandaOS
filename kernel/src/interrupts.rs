//! Interrupt handling for x86_64
//!
//! This module sets up the Interrupt Descriptor Table (IDT) and handles
//! various CPU exceptions and hardware interrupts.

use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

/// Initialize interrupt handling
pub fn init() {
    // SAFETY: This is called once during kernel init, we're setting up the IDT
    unsafe {
        let idt = &mut *core::ptr::addr_of_mut!(IDT);
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.double_fault.set_handler_fn(double_fault_handler);
        idt.load();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breakpoint_exception() {
        // Invoke a breakpoint exception
        x86_64::instructions::interrupts::int3();
    }
}
