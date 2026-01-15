//! Programmable Interrupt Controller (PIC) driver for x86_64
//!
//! The 8259 PIC (Programmable Interrupt Controller) manages hardware interrupts.
//! Modern x86_64 systems have two PICs in a master-slave configuration.
//!
//! ## Safety
//!
//! All PIC configuration is unsafe as it involves direct hardware I/O.
//!
//! ## Invariants
//!
//! - PIC must be initialized before enabling interrupts
//! - IRQs must be remapped to avoid conflicts with CPU exceptions (0-31)
//! - EOI must be sent after handling each hardware interrupt

use x86_64::instructions::port::Port;

/// Master PIC command port
const PIC1_COMMAND: u16 = 0x20;
/// Master PIC data port
const PIC1_DATA: u16 = 0x21;

/// Slave PIC command port
const PIC2_COMMAND: u16 = 0xA0;
/// Slave PIC data port
const PIC2_DATA: u16 = 0xA1;

/// Initialize command for PIC
const ICW1_INIT: u8 = 0x11;
/// 8086/88 mode
const ICW4_8086: u8 = 0x01;

/// End of interrupt command
const EOI: u8 = 0x20;

/// Initialize the PIC with remapped IRQ offsets
///
/// Remaps the PIC interrupts to avoid conflicts with CPU exceptions.
/// - Master PIC (IRQ 0-7) -> interrupts 32-39
/// - Slave PIC (IRQ 8-15) -> interrupts 40-47
///
/// # Safety
///
/// Must be called exactly once during kernel initialization before
/// enabling interrupts.
pub unsafe fn init() {
    let offset1 = 32; // Master PIC offset
    let offset2 = 40; // Slave PIC offset

    // SAFETY: We're initializing the PIC according to the specification
    unsafe {
        let mut cmd1 = Port::<u8>::new(PIC1_COMMAND);
        let mut data1 = Port::<u8>::new(PIC1_DATA);
        let mut cmd2 = Port::<u8>::new(PIC2_COMMAND);
        let mut data2 = Port::<u8>::new(PIC2_DATA);

        // Save masks
        let mask1 = data1.read();
        let mask2 = data2.read();

        // Start initialization sequence
        cmd1.write(ICW1_INIT);
        io_wait();
        cmd2.write(ICW1_INIT);
        io_wait();

        // Set vector offsets
        data1.write(offset1);
        io_wait();
        data2.write(offset2);
        io_wait();

        // Tell master PIC that there is a slave PIC at IRQ2
        data1.write(0x04);
        io_wait();
        // Tell slave PIC its cascade identity
        data2.write(0x02);
        io_wait();

        // Set 8086 mode
        data1.write(ICW4_8086);
        io_wait();
        data2.write(ICW4_8086);
        io_wait();

        // Restore masks
        data1.write(mask1);
        data2.write(mask2);
    }
}

/// Unmask an IRQ line
///
/// Enables a specific IRQ by clearing its mask bit.
///
/// # Safety
///
/// Must be called after PIC initialization.
/// IRQ must be < 16.
pub unsafe fn unmask_irq(irq: u8) {
    assert!(irq < 16, "IRQ must be < 16");

    let port = if irq < 8 { PIC1_DATA } else { PIC2_DATA };
    let value = if irq < 8 { irq } else { irq - 8 };

    // SAFETY: Caller guarantees PIC is initialized
    unsafe {
        let mut data_port = Port::<u8>::new(port);
        let mask = data_port.read();
        let new_mask = mask & !(1 << value);
        data_port.write(new_mask);
    }
}

/// Mask an IRQ line
///
/// Disables a specific IRQ by setting its mask bit.
///
/// # Safety
///
/// Must be called after PIC initialization.
/// IRQ must be < 16.
pub unsafe fn mask_irq(irq: u8) {
    assert!(irq < 16, "IRQ must be < 16");

    let port = if irq < 8 { PIC1_DATA } else { PIC2_DATA };
    let value = if irq < 8 { irq } else { irq - 8 };

    // SAFETY: Caller guarantees PIC is initialized
    unsafe {
        let mut data_port = Port::<u8>::new(port);
        let mask = data_port.read();
        let new_mask = mask | (1 << value);
        data_port.write(new_mask);
    }
}

/// Send End of Interrupt (EOI) to the PIC
///
/// Must be called at the end of every hardware interrupt handler.
///
/// # Safety
///
/// Must be called after PIC initialization.
/// IRQ must be < 16.
pub unsafe fn send_eoi(irq: u8) {
    assert!(irq < 16, "IRQ must be < 16");

    // SAFETY: Sending EOI to PIC is safe and required
    unsafe {
        // If IRQ came from slave PIC (>= 8), send EOI to both PICs
        if irq >= 8 {
            let mut slave_cmd = Port::<u8>::new(PIC2_COMMAND);
            slave_cmd.write(EOI);
        }

        // Always send EOI to master PIC
        let mut master_cmd = Port::<u8>::new(PIC1_COMMAND);
        master_cmd.write(EOI);
    }
}

/// Wait for I/O operation to complete
///
/// Uses port 0x80 (unused diagnostic port) to add a small delay.
#[inline]
fn io_wait() {
    // SAFETY: Writing to port 0x80 is safe and used for I/O delays
    unsafe {
        let mut port = Port::<u8>::new(0x80);
        port.write(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pic_constants() {
        assert_eq!(PIC1_COMMAND, 0x20);
        assert_eq!(PIC1_DATA, 0x21);
        assert_eq!(PIC2_COMMAND, 0xA0);
        assert_eq!(PIC2_DATA, 0xA1);
    }

    #[test]
    #[should_panic(expected = "IRQ must be < 16")]
    fn test_invalid_irq() {
        unsafe {
            unmask_irq(16);
        }
    }
}
