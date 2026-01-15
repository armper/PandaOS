//! Serial port driver for `x86_64`
//!
//! Provides a simple interface for serial communication via COM1-COM4 ports.
//! This is essential for kernel debugging and early boot logging.

#![cfg(feature = "hardware")]

use spin::Mutex;
use uart_16550::SerialPort;

/// Global serial port instance (COM1)
pub static SERIAL1: Mutex<Option<SerialPort>> = Mutex::new(None);

/// Initialize the serial port at the given base address
///
/// # Safety
///
/// The caller must ensure that `base` is a valid serial port I/O address
/// and that this function is called only once.
pub unsafe fn init() {
    // SAFETY: Caller guarantees this is a valid serial port address
    // and this function is called only once
    let mut serial = unsafe { SerialPort::new(0x3F8) }; // COM1
    serial.init();
    *SERIAL1.lock() = Some(serial);
}

/// Print to the serial port
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*))
    };
}

/// Print to the serial port with a newline
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    // Disable interrupts while writing to avoid deadlocks
    interrupts::without_interrupts(|| {
        if let Some(serial) = SERIAL1.lock().as_mut() {
            serial.write_fmt(args).expect("Serial write failed");
        }
    });
}

/// Read a byte from the serial port if available.
pub fn serial_read_byte() -> Option<u8> {
    use x86_64::instructions::interrupts;

    let mut byte = None;
    interrupts::without_interrupts(|| {
        if let Some(serial) = SERIAL1.lock().as_mut() {
            if let Ok(data) = serial.try_receive() {
                byte = Some(data);
            }
        }
    });

    byte
}

/// Write a raw byte to the serial port without UTF-8 translation.
pub fn write_byte_raw(byte: u8) {
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        if let Some(serial) = SERIAL1.lock().as_mut() {
            serial.send_raw(byte);
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_serial_module_compiles() {
        // This test ensures the module structure is correct
        // Actual hardware tests would require QEMU
    }
}
