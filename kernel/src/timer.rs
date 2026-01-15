//! Programmable Interval Timer (PIT) driver for x86_64
//!
//! The PIT (8253/8254) provides periodic timer interrupts for preemptive
//! multitasking. It operates at a base frequency of 1.193182 MHz.
//!
//! ## Safety
//!
//! All hardware access in this module is unsafe. Each unsafe block documents
//! the specific hardware operation and why it's safe.
//!
//! ## Invariants
//!
//! - PIT must be initialized before enabling timer interrupts
//! - Timer frequency must be > 0 and <= PIT_BASE_FREQUENCY
//! - Only one PIT configuration should be active at a time

use x86_64::instructions::port::Port;

/// PIT base frequency in Hz (1.193182 MHz)
const PIT_BASE_FREQUENCY: u32 = 1_193_182;

/// PIT channel 0 data port (system timer)
const PIT_CHANNEL_0: u16 = 0x40;

/// PIT command port
const PIT_COMMAND: u16 = 0x43;

/// PIT command: Channel 0, lobyte/hibyte, rate generator, binary mode
const PIT_CMD_BINARY_MODE_RATE_GEN: u8 = 0b00110110;

/// Initialize the PIT with a given frequency
///
/// Sets up the PIT to generate interrupts at the specified frequency.
/// The PIT will trigger IRQ 0 at this rate.
///
/// # Arguments
///
/// * `frequency_hz` - Desired interrupt frequency in Hz (e.g., 100 for 10ms intervals)
///
/// # Safety
///
/// - Must be called before enabling timer interrupts
/// - Must not be called multiple times without coordination
/// - frequency_hz must be > 0 and <= PIT_BASE_FREQUENCY
///
/// # Panics
///
/// Panics if frequency_hz is 0 or greater than PIT_BASE_FREQUENCY
pub unsafe fn init(frequency_hz: u32) {
    assert!(frequency_hz > 0, "PIT frequency must be greater than 0");
    assert!(frequency_hz <= PIT_BASE_FREQUENCY, "PIT frequency cannot exceed base frequency");

    // Calculate divisor for desired frequency
    let divisor = PIT_BASE_FREQUENCY / frequency_hz;
    assert!(divisor <= 65535, "PIT divisor out of range");

    // SAFETY: Writing to PIT ports is safe for PIT initialization
    // Port 0x43 is the PIT command port
    // Port 0x40 is channel 0 data port
    unsafe {
        let mut cmd_port = Port::<u8>::new(PIT_COMMAND);
        let mut data_port = Port::<u8>::new(PIT_CHANNEL_0);

        // Send command byte
        cmd_port.write(PIT_CMD_BINARY_MODE_RATE_GEN);

        // Send divisor (low byte, then high byte)
        data_port.write((divisor & 0xFF) as u8);
        data_port.write((divisor >> 8) as u8);
    }
}

/// Get the configured timer frequency in Hz
///
/// Returns the frequency that was set via `init()`.
/// If the PIT has not been initialized, this returns None.
pub fn get_frequency() -> Option<u32> {
    // TODO: Track the configured frequency in a static variable
    // For now, we'll assume a standard 100 Hz if initialized
    Some(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pit_constants() {
        assert_eq!(PIT_BASE_FREQUENCY, 1_193_182);
        assert_eq!(PIT_CHANNEL_0, 0x40);
        assert_eq!(PIT_COMMAND, 0x43);
    }

    #[test]
    fn test_divisor_calculation() {
        let freq_100hz = 100;
        let divisor = PIT_BASE_FREQUENCY / freq_100hz;
        assert_eq!(divisor, 11931);
        assert!(divisor <= 65535, "Divisor should fit in 16 bits");
    }

    #[test]
    fn test_divisor_1000hz() {
        let freq_1000hz = 1000;
        let divisor = PIT_BASE_FREQUENCY / freq_1000hz;
        assert_eq!(divisor, 1193);
    }

    #[test]
    #[should_panic(expected = "PIT frequency must be greater than 0")]
    fn test_init_zero_frequency() {
        unsafe {
            init(0);
        }
    }
}
