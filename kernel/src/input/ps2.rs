//! PS/2 Keyboard Driver for x86_64
//!
//! This module provides a driver for PS/2 keyboards through the i8042 controller.
//! It handles IRQ1 interrupts and decodes scancodes to ASCII characters.
//!
//! ## Architecture
//!
//! - i8042 Controller: ports 0x60 (data) and 0x64 (status/command)
//! - IRQ1 (interrupt 33) for keyboard events
//! - Ring buffer for scancode storage (no heap allocation)
//! - Scancode Set 1 decoder with modifier key tracking
//!
//! ## Safety
//!
//! All hardware I/O operations are unsafe and documented with SAFETY comments.

use panda_hal::ringbuffer::RingBuffer;
use spin::Mutex;
use x86_64::instructions::port::Port;

/// PS/2 data port (0x60)
const PS2_DATA_PORT: u16 = 0x60;
/// PS/2 status/command port (0x64)
const PS2_STATUS_PORT: u16 = 0x64;

/// Size of scancode ring buffer
const SCANCODE_BUFFER_SIZE: usize = 128;

/// PS/2 controller status register bits
const STATUS_OUTPUT_FULL: u8 = 0x01;

/// Global scancode ring buffer
static SCANCODE_BUFFER: Mutex<RingBuffer<u8, SCANCODE_BUFFER_SIZE>> =
    Mutex::new(RingBuffer::new());

/// Keyboard state for tracking modifiers
struct KeyboardState {
    shift_pressed: bool,
    ctrl_pressed: bool,
}

impl KeyboardState {
    const fn new() -> Self {
        Self { shift_pressed: false, ctrl_pressed: false }
    }
}

/// Global keyboard state
static KEYBOARD_STATE: Mutex<KeyboardState> = Mutex::new(KeyboardState::new());

/// Initialize PS/2 keyboard controller
///
/// # Safety
///
/// Must be called once during kernel initialization after PIC is initialized.
pub unsafe fn init() {
    // SAFETY: Caller guarantees this is called once during init
    unsafe {
        let mut cmd_port = Port::<u8>::new(PS2_STATUS_PORT);
        let mut data_port = Port::<u8>::new(PS2_DATA_PORT);

        // Disable first PS/2 port during setup
        cmd_port.write(0xAD);
        io_wait();

        // Flush output buffer
        let _ = data_port.read();

        // Enable first PS/2 port
        cmd_port.write(0xAE);
        io_wait();

        // Enable keyboard interrupt (IRQ1)
        crate::pic::unmask_irq(1);
    }
}

/// Handle keyboard interrupt (called from IRQ1 handler)
///
/// Reads scancode from PS/2 controller and stores in ring buffer.
///
/// # Safety
///
/// Must be called only from IRQ1 interrupt handler.
pub unsafe fn handle_keyboard_interrupt() {
    // SAFETY: Called from IRQ1 handler, reading from hardware port
    unsafe {
        let mut status_port = Port::<u8>::new(PS2_STATUS_PORT);
        let mut data_port = Port::<u8>::new(PS2_DATA_PORT);

        // Read all available scancodes
        while status_port.read() & STATUS_OUTPUT_FULL != 0 {
            let scancode = data_port.read();

            // Store in ring buffer
            SCANCODE_BUFFER.lock().push(scancode);

            #[cfg(feature = "kbd-log")]
            {
                // Rate-limited debug logging
                static mut LOG_COUNT: usize = 0;
                LOG_COUNT += 1;
                if LOG_COUNT % 10 == 0 {
                    panda_hal::serial_println!("[KBD] scancode: {:#04x}", scancode);
                }
            }
        }
    }
}

/// Try to read next scancode from buffer (non-blocking)
pub fn try_read_scancode() -> Option<u8> {
    SCANCODE_BUFFER.lock().pop()
}

/// Decode scancode to ASCII character
///
/// Returns None for non-character keys (modifiers, function keys, etc.)
pub fn decode_scancode(scancode: u8) -> Option<u8> {
    let mut state = KEYBOARD_STATE.lock();

    // Check for key release (high bit set)
    let is_release = scancode & 0x80 != 0;
    let key_code = scancode & 0x7F;

    // Handle modifier keys
    match key_code {
        0x2A | 0x36 => {
            // Left Shift (0x2A) or Right Shift (0x36)
            state.shift_pressed = !is_release;
            return None;
        }
        0x1D => {
            // Left Ctrl (0x1D)
            state.ctrl_pressed = !is_release;
            return None;
        }
        _ => {}
    }

    // Only process key press events (not releases)
    if is_release {
        return None;
    }

    // Handle Ctrl combinations
    if state.ctrl_pressed {
        match key_code {
            0x2E => return Some(0x03), // Ctrl+C
            0x2C => return Some(0x1A), // Ctrl+Z
            _ => return None,
        }
    }

    // Map scancode to ASCII
    decode_key_to_ascii(key_code, state.shift_pressed)
}

/// Map scancode to ASCII character
fn decode_key_to_ascii(scancode: u8, shift: bool) -> Option<u8> {
    match scancode {
        // Numbers row
        0x02 => Some(if shift { b'!' } else { b'1' }),
        0x03 => Some(if shift { b'@' } else { b'2' }),
        0x04 => Some(if shift { b'#' } else { b'3' }),
        0x05 => Some(if shift { b'$' } else { b'4' }),
        0x06 => Some(if shift { b'%' } else { b'5' }),
        0x07 => Some(if shift { b'^' } else { b'6' }),
        0x08 => Some(if shift { b'&' } else { b'7' }),
        0x09 => Some(if shift { b'*' } else { b'8' }),
        0x0A => Some(if shift { b'(' } else { b'9' }),
        0x0B => Some(if shift { b')' } else { b'0' }),
        0x0C => Some(if shift { b'_' } else { b'-' }),
        0x0D => Some(if shift { b'+' } else { b'=' }),

        // Letters - Q row
        0x10 => Some(if shift { b'Q' } else { b'q' }),
        0x11 => Some(if shift { b'W' } else { b'w' }),
        0x12 => Some(if shift { b'E' } else { b'e' }),
        0x13 => Some(if shift { b'R' } else { b'r' }),
        0x14 => Some(if shift { b'T' } else { b't' }),
        0x15 => Some(if shift { b'Y' } else { b'y' }),
        0x16 => Some(if shift { b'U' } else { b'u' }),
        0x17 => Some(if shift { b'I' } else { b'i' }),
        0x18 => Some(if shift { b'O' } else { b'o' }),
        0x19 => Some(if shift { b'P' } else { b'p' }),
        0x1A => Some(if shift { b'{' } else { b'[' }),
        0x1B => Some(if shift { b'}' } else { b']' }),

        // Letters - A row
        0x1E => Some(if shift { b'A' } else { b'a' }),
        0x1F => Some(if shift { b'S' } else { b's' }),
        0x20 => Some(if shift { b'D' } else { b'd' }),
        0x21 => Some(if shift { b'F' } else { b'f' }),
        0x22 => Some(if shift { b'G' } else { b'g' }),
        0x23 => Some(if shift { b'H' } else { b'h' }),
        0x24 => Some(if shift { b'J' } else { b'j' }),
        0x25 => Some(if shift { b'K' } else { b'k' }),
        0x26 => Some(if shift { b'L' } else { b'l' }),
        0x27 => Some(if shift { b':' } else { b';' }),
        0x28 => Some(if shift { b'"' } else { b'\'' }),

        // Letters - Z row
        0x2C => Some(if shift { b'Z' } else { b'z' }),
        0x2D => Some(if shift { b'X' } else { b'x' }),
        0x2E => Some(if shift { b'C' } else { b'c' }),
        0x2F => Some(if shift { b'V' } else { b'v' }),
        0x30 => Some(if shift { b'B' } else { b'b' }),
        0x31 => Some(if shift { b'N' } else { b'n' }),
        0x32 => Some(if shift { b'M' } else { b'm' }),
        0x33 => Some(if shift { b'<' } else { b',' }),
        0x34 => Some(if shift { b'>' } else { b'.' }),
        0x35 => Some(if shift { b'?' } else { b'/' }),

        // Special keys
        0x0E => Some(0x08),   // Backspace
        0x1C => Some(b'\n'),  // Enter
        0x39 => Some(b' '),   // Space
        0x0F => Some(b'\t'),  // Tab
        0x2B => Some(if shift { b'|' } else { b'\\' }), // Backslash

        // Ignore other keys
        _ => None,
    }
}

/// Wait for I/O operation to complete
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
    fn test_decode_letter_a() {
        // Scancode 0x1E = 'a' key
        let ascii = decode_key_to_ascii(0x1E, false);
        assert_eq!(ascii, Some(b'a'));

        let ascii_shift = decode_key_to_ascii(0x1E, true);
        assert_eq!(ascii_shift, Some(b'A'));
    }

    #[test]
    fn test_decode_number_1() {
        // Scancode 0x02 = '1' key
        let ascii = decode_key_to_ascii(0x02, false);
        assert_eq!(ascii, Some(b'1'));

        let ascii_shift = decode_key_to_ascii(0x02, true);
        assert_eq!(ascii_shift, Some(b'!'));
    }

    #[test]
    fn test_decode_special_keys() {
        assert_eq!(decode_key_to_ascii(0x0E, false), Some(0x08)); // Backspace
        assert_eq!(decode_key_to_ascii(0x1C, false), Some(b'\n')); // Enter
        assert_eq!(decode_key_to_ascii(0x39, false), Some(b' ')); // Space
    }
}
