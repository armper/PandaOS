//! Unified console abstraction for PandaOS
//!
//! This module provides a unified console interface that can write to both
//! serial and VGA outputs simultaneously. This ensures boot messages are visible
//! regardless of which output the user is monitoring.
//!
//! ## Usage
//!
//! ```rust,ignore
//! console_println!("Boot message");  // Prints to both serial and VGA
//! ```

use core::fmt;

/// Print to console (both serial and VGA if enabled)
#[macro_export]
macro_rules! console_print {
    ($($arg:tt)*) => {{
        // Always print to serial
        serial_print!($($arg)*);

        // Print to VGA if vga-console feature is enabled
        #[cfg(feature = "vga-console")]
        {
            use core::fmt::Write;
            $crate::console::vga_print(format_args!($($arg)*));
        }
    }};
}

/// Print to console with newline (both serial and VGA if enabled)
#[macro_export]
macro_rules! console_println {
    () => ($crate::console_print!("\n"));
    ($($arg:tt)*) => {{
        $crate::console_print!("{}\n", format_args!($($arg)*));
    }};
}

/// Print to VGA (when vga-console feature is enabled)
#[cfg(feature = "vga-console")]
pub fn vga_print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        if let Some(writer) = panda_hal::vga::WRITER.lock().as_mut() {
            let _ = writer.write_fmt(args);
        }
    });
}

/// Console trait for unified output
///
/// This trait provides a common interface for writing to both serial and VGA
/// outputs without requiring heap allocation.
pub trait Console: fmt::Write {
    /// Write a string to the console
    fn write_str(&mut self, s: &str) -> fmt::Result;
}

/// Dual console writer that outputs to both serial and VGA
#[cfg(feature = "vga-console")]
pub struct DualConsole;

#[cfg(feature = "vga-console")]
impl fmt::Write for DualConsole {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // Write to serial
        serial_print!("{}", s);

        // Write to VGA
        vga_print(format_args!("{}", s));

        Ok(())
    }
}

/// Print the boot banner to all available consoles
pub fn print_boot_banner() {
    console_println!("╔════════════════════════════════════════════════════════════════╗");
    console_println!("║              PandaOS - Unix-like x86_64 Kernel                 ║");
    console_println!(
        "║                    Version {}                          ║",
        env!("CARGO_PKG_VERSION")
    );
    console_println!("╚════════════════════════════════════════════════════════════════╝");
    console_println!();
}

/// Print the ready marker to indicate successful boot
pub fn print_ready_marker() {
    console_println!();
    console_println!("════════════════════════════════════════════════════════════════");
    console_println!("                        PANDA READY");
    console_println!("════════════════════════════════════════════════════════════════");
    console_println!();
}
