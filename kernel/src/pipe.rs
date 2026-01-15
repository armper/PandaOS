//! Pipe implementation for inter-process communication
//!
//! This module provides Unix-like pipes with a fixed-size ring buffer.
//! Pipes support multiple readers and writers through reference counting.
//!
//! ## Invariants
//!
//! - Each pipe has a fixed 4KB buffer
//! - Pipes are reference-counted by open read/write ends
//! - When last writer closes: readers get EOF when buffer empty
//! - When last reader closes: writers get EPIPE on write
//! - Maximum 16 pipes can exist concurrently
//! - Pipe operations return EAGAIN when buffer full/empty (non-blocking)

use crate::syscall::ErrorCode;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

/// Pipe buffer size (4KB)
const PIPE_BUFFER_SIZE: usize = 4096;

/// Maximum number of concurrent pipes
const MAX_PIPES: usize = 16;

/// A single pipe with reference-counted ends
#[derive(Debug)]
pub struct Pipe {
    /// Ring buffer for data
    buffer: [u8; PIPE_BUFFER_SIZE],
    /// Read position in buffer
    read_pos: usize,
    /// Write position in buffer
    write_pos: usize,
    /// Number of bytes currently in buffer
    count: usize,
    /// Number of open read ends
    read_refcount: usize,
    /// Number of open write ends
    write_refcount: usize,
}

impl Pipe {
    /// Create a new empty pipe
    const fn new() -> Self {
        Self {
            buffer: [0u8; PIPE_BUFFER_SIZE],
            read_pos: 0,
            write_pos: 0,
            count: 0,
            read_refcount: 0,
            write_refcount: 0,
        }
    }

    /// Check if pipe is allocated (has any open ends)
    const fn is_allocated(&self) -> bool {
        self.read_refcount > 0 || self.write_refcount > 0
    }

    /// Check if buffer is empty
    const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Check if buffer is full
    const fn is_full(&self) -> bool {
        self.count == PIPE_BUFFER_SIZE
    }

    /// Open a read end (increment read refcount)
    fn open_read(&mut self) {
        self.read_refcount = self.read_refcount.saturating_add(1);
    }

    /// Open a write end (increment write refcount)
    fn open_write(&mut self) {
        self.write_refcount = self.write_refcount.saturating_add(1);
    }

    /// Close a read end (decrement read refcount)
    fn close_read(&mut self) {
        self.read_refcount = self.read_refcount.saturating_sub(1);
    }

    /// Close a write end (decrement write refcount)
    fn close_write(&mut self) {
        self.write_refcount = self.write_refcount.saturating_sub(1);
    }

    /// Write data to the pipe buffer
    ///
    /// Returns the number of bytes written, or EAGAIN if buffer is full.
    /// Returns EPIPE if no readers are open.
    fn write(&mut self, data: &[u8]) -> Result<usize, ErrorCode> {
        // Check if any readers are open
        if self.read_refcount == 0 {
            return Err(ErrorCode::EPIPE);
        }

        // Return EAGAIN if buffer is full (non-blocking)
        if self.is_full() {
            return Err(ErrorCode::EAGAIN);
        }

        // Write as much as possible without overflowing
        let available_space = PIPE_BUFFER_SIZE - self.count;
        let to_write = data.len().min(available_space);

        for i in 0..to_write {
            self.buffer[self.write_pos] = data[i];
            self.write_pos = (self.write_pos + 1) % PIPE_BUFFER_SIZE;
        }

        self.count += to_write;
        Ok(to_write)
    }

    /// Read data from the pipe buffer
    ///
    /// Returns the number of bytes read (0 for EOF).
    /// Returns EAGAIN if buffer is empty and writers exist.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ErrorCode> {
        // EOF: no writers and buffer empty
        if self.write_refcount == 0 && self.is_empty() {
            return Ok(0);
        }

        // Return EAGAIN if buffer is empty but writers exist (non-blocking)
        if self.is_empty() {
            return Err(ErrorCode::EAGAIN);
        }

        // Read as much as possible
        let to_read = buf.len().min(self.count);

        for i in 0..to_read {
            buf[i] = self.buffer[self.read_pos];
            self.read_pos = (self.read_pos + 1) % PIPE_BUFFER_SIZE;
        }

        self.count -= to_read;
        Ok(to_read)
    }
}

/// Pipe ID type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipeId(usize);

impl PipeId {
    /// Get the raw pipe ID
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

/// Global pipe pool
struct PipePool {
    pipes: [Pipe; MAX_PIPES],
}

impl PipePool {
    /// Create a new pipe pool
    const fn new() -> Self {
        Self { pipes: [const { Pipe::new() }; MAX_PIPES] }
    }

    /// Allocate a new pipe and return its ID
    fn allocate(&mut self) -> Result<PipeId, ErrorCode> {
        for (i, pipe) in self.pipes.iter_mut().enumerate() {
            if !pipe.is_allocated() {
                // Initialize the pipe with both ends open
                pipe.open_read();
                pipe.open_write();
                return Ok(PipeId(i));
            }
        }
        Err(ErrorCode::EMFILE)
    }

    /// Get a pipe by ID
    fn get(&mut self, id: PipeId) -> Option<&mut Pipe> {
        let pipe = self.pipes.get_mut(id.0)?;
        if pipe.is_allocated() {
            Some(pipe)
        } else {
            None
        }
    }

    /// Open a read end for an existing pipe
    fn open_read_end(&mut self, id: PipeId) -> Result<(), ErrorCode> {
        let pipe = self.get(id).ok_or(ErrorCode::EBADF)?;
        pipe.open_read();
        Ok(())
    }

    /// Open a write end for an existing pipe
    fn open_write_end(&mut self, id: PipeId) -> Result<(), ErrorCode> {
        let pipe = self.get(id).ok_or(ErrorCode::EBADF)?;
        pipe.open_write();
        Ok(())
    }

    /// Close a read end
    fn close_read_end(&mut self, id: PipeId) -> Result<(), ErrorCode> {
        let pipe = self.get(id).ok_or(ErrorCode::EBADF)?;
        pipe.close_read();
        Ok(())
    }

    /// Close a write end
    fn close_write_end(&mut self, id: PipeId) -> Result<(), ErrorCode> {
        let pipe = self.get(id).ok_or(ErrorCode::EBADF)?;
        pipe.close_write();
        Ok(())
    }

    /// Write to a pipe
    fn write(&mut self, id: PipeId, data: &[u8]) -> Result<usize, ErrorCode> {
        let pipe = self.get(id).ok_or(ErrorCode::EBADF)?;
        pipe.write(data)
    }

    /// Read from a pipe
    fn read(&mut self, id: PipeId, buf: &mut [u8]) -> Result<usize, ErrorCode> {
        let pipe = self.get(id).ok_or(ErrorCode::EBADF)?;
        pipe.read(buf)
    }
}

/// Global pipe pool instance
static PIPE_POOL: Mutex<PipePool> = Mutex::new(PipePool::new());

/// Create a new pipe
///
/// Returns (read_end_id, write_end_id)
pub fn pipe_create() -> Result<(PipeId, PipeId), ErrorCode> {
    let mut pool = PIPE_POOL.lock();
    let id = pool.allocate()?;
    Ok((id, id))
}

/// Open a read end (for dup/fork)
pub fn pipe_open_read_end(id: PipeId) -> Result<(), ErrorCode> {
    let mut pool = PIPE_POOL.lock();
    pool.open_read_end(id)
}

/// Open a write end (for dup/fork)
pub fn pipe_open_write_end(id: PipeId) -> Result<(), ErrorCode> {
    let mut pool = PIPE_POOL.lock();
    pool.open_write_end(id)
}

/// Close a read end
pub fn pipe_close_read(id: PipeId) -> Result<(), ErrorCode> {
    let mut pool = PIPE_POOL.lock();
    pool.close_read_end(id)
}

/// Close a write end
pub fn pipe_close_write(id: PipeId) -> Result<(), ErrorCode> {
    let mut pool = PIPE_POOL.lock();
    pool.close_write_end(id)
}

/// Write to a pipe
pub fn pipe_write(id: PipeId, data: &[u8]) -> Result<usize, ErrorCode> {
    let mut pool = PIPE_POOL.lock();
    pool.write(id, data)
}

/// Read from a pipe
pub fn pipe_read(id: PipeId, buf: &mut [u8]) -> Result<usize, ErrorCode> {
    let mut pool = PIPE_POOL.lock();
    pool.read(id, buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipe_write_read() {
        let mut pipe = Pipe::new();
        pipe.open_read();
        pipe.open_write();

        let data = b"hello world";
        let written = pipe.write(data).expect("write should succeed");
        assert_eq!(written, data.len());

        let mut buf = [0u8; 64];
        let read = pipe.read(&mut buf).expect("read should succeed");
        assert_eq!(read, data.len());
        assert_eq!(&buf[..read], data);
    }

    #[test]
    fn test_pipe_eof_when_writer_closes() {
        let mut pipe = Pipe::new();
        pipe.open_read();
        pipe.open_write();

        let data = b"test";
        pipe.write(data).expect("write should succeed");

        // Close writer
        pipe.close_write();

        // Read existing data
        let mut buf = [0u8; 64];
        let read = pipe.read(&mut buf).expect("read should succeed");
        assert_eq!(read, data.len());

        // Next read should return EOF (0 bytes)
        let read = pipe.read(&mut buf).expect("read should succeed");
        assert_eq!(read, 0);
    }

    #[test]
    fn test_pipe_epipe_when_reader_closes() {
        let mut pipe = Pipe::new();
        pipe.open_read();
        pipe.open_write();

        // Close reader
        pipe.close_read();

        // Write should return EPIPE
        let data = b"test";
        let err = pipe.write(data).unwrap_err();
        assert_eq!(err, ErrorCode::EPIPE);
    }

    #[test]
    fn test_pipe_eagain_on_full() {
        let mut pipe = Pipe::new();
        pipe.open_read();
        pipe.open_write();

        // Fill the buffer
        let data = [0xAA; PIPE_BUFFER_SIZE];
        let written = pipe.write(&data).expect("write should succeed");
        assert_eq!(written, PIPE_BUFFER_SIZE);

        // Next write should return EAGAIN
        let err = pipe.write(b"x").unwrap_err();
        assert_eq!(err, ErrorCode::EAGAIN);
    }

    #[test]
    fn test_pipe_eagain_on_empty() {
        let mut pipe = Pipe::new();
        pipe.open_read();
        pipe.open_write();

        // Read from empty buffer should return EAGAIN
        let mut buf = [0u8; 64];
        let err = pipe.read(&mut buf).unwrap_err();
        assert_eq!(err, ErrorCode::EAGAIN);
    }

    #[test]
    fn test_pipe_refcounting() {
        let mut pipe = Pipe::new();
        assert!(!pipe.is_allocated());

        pipe.open_read();
        assert!(pipe.is_allocated());

        pipe.open_write();
        assert!(pipe.is_allocated());

        pipe.close_read();
        assert!(pipe.is_allocated());

        pipe.close_write();
        assert!(!pipe.is_allocated());
    }

    #[test]
    fn test_pipe_pool_allocation() {
        // Create a pipe pool for testing
        let mut pool = PipePool::new();

        // Allocate a pipe
        let id = pool.allocate().expect("allocate should succeed");

        // Write and read
        let written = pool.write(id, b"test").expect("write should succeed");
        assert_eq!(written, 4);

        let mut buf = [0u8; 64];
        let read = pool.read(id, &mut buf).expect("read should succeed");
        assert_eq!(read, 4);
        assert_eq!(&buf[..4], b"test");

        // Close both ends
        pool.close_read_end(id).expect("close should succeed");
        pool.close_write_end(id).expect("close should succeed");

        // Pipe should be deallocated
        assert!(pool.get(id).is_none());
    }

    #[test]
    fn test_pipe_wraparound() {
        let mut pipe = Pipe::new();
        pipe.open_read();
        pipe.open_write();

        // Write some data
        let data1 = [1u8; 100];
        pipe.write(&data1).expect("write should succeed");

        // Read some data
        let mut buf = [0u8; 50];
        pipe.read(&mut buf).expect("read should succeed");
        assert_eq!(&buf, &data1[..50]);

        // Write more data to cause wraparound
        let data2 = [2u8; 100];
        pipe.write(&data2).expect("write should succeed");

        // Read remaining data
        let mut buf = [0u8; 150];
        let read = pipe.read(&mut buf).expect("read should succeed");
        assert_eq!(read, 150);
        assert_eq!(&buf[..50], &data1[50..]);
        assert_eq!(&buf[50..150], &data2);
    }
}
