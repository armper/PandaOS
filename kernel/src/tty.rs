//! TTY (Terminal) Subsystem for PandaOS
//!
//! This module implements a minimal TTY layer between raw device input (serial)
//! and user programs. It provides:
//! - Line buffering (canonical mode)
//! - Echo
//! - Special character handling (backspace, Ctrl+C)
//! - Clean separation between raw device I/O and cooked terminal I/O
//!
//! ## Architecture
//!
//! ```text
//! Serial Device → TTY Input Handler → Line Buffer → sys_read(stdin)
//!                       ↓
//!                   Echo Output → Serial Device
//! ```
//!
//! ## Invariants
//!
//! - At most one TTY instance (single controlling terminal)
//! - Line buffer contains uncommitted input until newline
//! - Committed lines are delivered to readers in FIFO order
//! - Echo is synchronous with input processing
//! - Ctrl+C always clears input buffer and sends SIGINT

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;

/// Maximum size of a single line in the TTY buffer
const MAX_LINE_LEN: usize = 256;

/// TTY state and line buffer
pub struct Tty {
    /// Current line being edited (not yet committed)
    current_line: Vec<u8>,
    /// Queue of completed lines ready to be read
    completed_lines: VecDeque<Vec<u8>>,
    /// Echo enabled flag
    echo: bool,
}

impl Tty {
    /// Create a new TTY with echo enabled
    pub const fn new() -> Self {
        Self { current_line: Vec::new(), completed_lines: VecDeque::new(), echo: true }
    }

    /// Process a single input byte from the device
    ///
    /// Returns true if a signal should be sent to the foreground process group
    pub fn input_byte(&mut self, byte: u8) -> TtyAction {
        match byte {
            // Ctrl+C (ETX)
            0x03 => {
                // Clear current line
                self.current_line.clear();
                // Echo ^C\n
                if self.echo {
                    panda_hal::serial::write_byte_raw(b'^');
                    panda_hal::serial::write_byte_raw(b'C');
                    panda_hal::serial::write_byte_raw(b'\r');
                    panda_hal::serial::write_byte_raw(b'\n');
                }
                TtyAction::SendSignal
            }
            // Backspace (BS or DEL)
            0x08 | 0x7F => {
                if !self.current_line.is_empty() {
                    self.current_line.pop();
                    // Echo backspace sequence: BS, space, BS
                    if self.echo {
                        panda_hal::serial::write_byte_raw(0x08);
                        panda_hal::serial::write_byte_raw(b' ');
                        panda_hal::serial::write_byte_raw(0x08);
                    }
                }
                TtyAction::None
            }
            // Newline (LF or CR)
            0x0A | 0x0D => {
                // Echo newline as CR+LF
                if self.echo {
                    panda_hal::serial::write_byte_raw(b'\r');
                    panda_hal::serial::write_byte_raw(b'\n');
                }
                // Commit current line
                let mut line = core::mem::take(&mut self.current_line);
                line.push(b'\n'); // Add newline to the committed line
                self.completed_lines.push_back(line);
                TtyAction::LineReady
            }
            // Normal printable characters
            _ => {
                // Only accept printable ASCII
                if (0x20..=0x7E).contains(&byte) {
                    if self.current_line.len() < MAX_LINE_LEN {
                        self.current_line.push(byte);
                        // Echo character
                        if self.echo {
                            panda_hal::serial::write_byte_raw(byte);
                        }
                    }
                }
                TtyAction::None
            }
        }
    }

    /// Read from completed lines into user buffer
    ///
    /// Returns number of bytes read, or None if no data available
    pub fn read(&mut self, buf: &mut [u8]) -> Option<usize> {
        if buf.is_empty() {
            return Some(0);
        }

        let mut total_read = 0;

        while total_read < buf.len() {
            // Get next line if available
            if let Some(line) = self.completed_lines.front_mut() {
                let to_copy = core::cmp::min(buf.len() - total_read, line.len());
                buf[total_read..total_read + to_copy].copy_from_slice(&line[..to_copy]);
                total_read += to_copy;

                // Remove consumed bytes from line
                line.drain(..to_copy);

                // If line is fully consumed, remove it
                if line.is_empty() {
                    self.completed_lines.pop_front();
                }

                // If we've consumed a newline, return what we have
                if total_read > 0 && buf[total_read - 1] == b'\n' {
                    break;
                }
            } else {
                // No more data available
                break;
            }
        }

        if total_read > 0 {
            Some(total_read)
        } else {
            None
        }
    }

    /// Check if there's data available to read
    pub fn has_data(&self) -> bool {
        !self.completed_lines.is_empty()
    }

    /// Clear all buffers (used on Ctrl+C)
    pub fn clear(&mut self) {
        self.current_line.clear();
        self.completed_lines.clear();
    }
}

/// Action to take after processing TTY input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtyAction {
    /// No special action needed
    None,
    /// A complete line is ready for reading
    LineReady,
    /// Send SIGINT to foreground process group
    SendSignal,
}

/// Global TTY instance
pub static GLOBAL_TTY: Mutex<Tty> = Mutex::new(Tty::new());

/// Process input from serial device into TTY
///
/// Should be called from keyboard interrupt handler or serial polling loop
pub fn tty_input_byte(byte: u8) -> TtyAction {
    GLOBAL_TTY.lock().input_byte(byte)
}

/// Read from TTY into buffer (non-blocking)
///
/// Returns Some(n) with bytes read if data available, None if would block
pub fn tty_read(buf: &mut [u8]) -> Option<usize> {
    GLOBAL_TTY.lock().read(buf)
}

/// Check if TTY has data available
pub fn tty_has_data() -> bool {
    GLOBAL_TTY.lock().has_data()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tty_line_buffering() {
        let mut tty = Tty::new();

        // Type "hello" without newline
        assert_eq!(tty.input_byte(b'h'), TtyAction::None);
        assert_eq!(tty.input_byte(b'e'), TtyAction::None);
        assert_eq!(tty.input_byte(b'l'), TtyAction::None);
        assert_eq!(tty.input_byte(b'l'), TtyAction::None);
        assert_eq!(tty.input_byte(b'o'), TtyAction::None);

        // Should have no completed lines yet
        assert!(!tty.has_data());

        // Press enter
        assert_eq!(tty.input_byte(b'\n'), TtyAction::LineReady);

        // Now should have data
        assert!(tty.has_data());

        // Read the line
        let mut buf = [0u8; 10];
        let n = tty.read(&mut buf).unwrap();
        assert_eq!(n, 6);
        assert_eq!(&buf[..n], b"hello\n");
    }

    #[test]
    fn test_tty_backspace() {
        let mut tty = Tty::new();

        tty.input_byte(b'h');
        tty.input_byte(b'i');
        tty.input_byte(0x08); // backspace
        tty.input_byte(b'a');
        tty.input_byte(b'\n');

        let mut buf = [0u8; 10];
        let n = tty.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"ha\n");
    }

    #[test]
    fn test_tty_ctrlc() {
        let mut tty = Tty::new();

        tty.input_byte(b'h');
        tty.input_byte(b'e');
        tty.input_byte(b'l');

        // Ctrl+C should clear buffer and return SendSignal
        assert_eq!(tty.input_byte(0x03), TtyAction::SendSignal);

        // Current line should be cleared
        assert_eq!(tty.current_line.len(), 0);

        // No completed lines
        assert!(!tty.has_data());
    }

    #[test]
    fn test_tty_multiple_lines() {
        let mut tty = Tty::new();

        // Type first line
        tty.input_byte(b'a');
        tty.input_byte(b'\n');

        // Type second line
        tty.input_byte(b'b');
        tty.input_byte(b'\n');

        // Read first line
        let mut buf = [0u8; 10];
        let n = tty.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"a\n");

        // Read second line
        let n = tty.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"b\n");
    }
}
