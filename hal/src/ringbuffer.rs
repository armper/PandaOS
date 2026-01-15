//! Ring buffer implementation for kernel logging
//!
//! A fixed-size circular buffer with pure logic, testable on host.

use core::fmt;

/// A circular ring buffer
#[derive(Debug)]
pub struct RingBuffer<T, const N: usize> {
    data: [Option<T>; N],
    head: usize,
    tail: usize,
    count: usize,
}

impl<T, const N: usize> RingBuffer<T, N> {
    /// Create a new empty ring buffer
    pub const fn new() -> Self {
        Self { data: [const { None }; N], head: 0, tail: 0, count: 0 }
    }

    /// Push an item to the back of the buffer
    ///
    /// If the buffer is full, the oldest item is overwritten
    pub fn push(&mut self, item: T) {
        self.data[self.tail] = Some(item);
        self.tail = (self.tail + 1) % N;

        if self.count == N {
            // Buffer is full, move head forward
            self.head = (self.head + 1) % N;
        } else {
            self.count += 1;
        }
    }

    /// Pop an item from the front of the buffer
    pub fn pop(&mut self) -> Option<T> {
        if self.count == 0 {
            return None;
        }

        let item = self.data[self.head].take();
        self.head = (self.head + 1) % N;
        self.count -= 1;
        item
    }

    /// Get the number of items in the buffer
    pub const fn len(&self) -> usize {
        self.count
    }

    /// Check if the buffer is empty
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Check if the buffer is full
    pub const fn is_full(&self) -> bool {
        self.count == N
    }

    /// Get the capacity of the buffer
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Clear all items from the buffer
    pub fn clear(&mut self) {
        while self.pop().is_some() {}
    }
}

impl<T: Copy, const N: usize> RingBuffer<T, N> {
    /// Peek at the front item without removing it
    pub fn peek(&self) -> Option<T> {
        if self.count == 0 {
            None
        } else {
            self.data[self.head]
        }
    }
}

impl<T, const N: usize> Default for RingBuffer<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: fmt::Display, const N: usize> fmt::Display for RingBuffer<T, N>
where
    T: Copy,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        let mut idx = self.head;
        for i in 0..self.count {
            if i > 0 {
                write!(f, ", ")?;
            }
            if let Some(ref item) = self.data[idx] {
                write!(f, "{item}")?;
            }
            idx = (idx + 1) % N;
        }
        write!(f, "]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_creation() {
        let buffer: RingBuffer<i32, 4> = RingBuffer::new();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        assert!(!buffer.is_full());
        assert_eq!(buffer.capacity(), 4);
    }

    #[test]
    fn test_ring_buffer_push_pop() {
        let mut buffer: RingBuffer<i32, 4> = RingBuffer::new();

        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.pop(), Some(1));
        assert_eq!(buffer.pop(), Some(2));
        assert_eq!(buffer.pop(), Some(3));
        assert_eq!(buffer.pop(), None);
    }

    #[test]
    fn test_ring_buffer_wraparound() {
        let mut buffer: RingBuffer<i32, 3> = RingBuffer::new();

        buffer.push(1);
        buffer.push(2);
        buffer.push(3);
        buffer.push(4); // Overwrites 1

        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.pop(), Some(2));
        assert_eq!(buffer.pop(), Some(3));
        assert_eq!(buffer.pop(), Some(4));
        assert_eq!(buffer.pop(), None);
    }

    #[test]
    fn test_ring_buffer_full() {
        let mut buffer: RingBuffer<i32, 2> = RingBuffer::new();

        buffer.push(1);
        assert!(!buffer.is_full());
        buffer.push(2);
        assert!(buffer.is_full());

        buffer.push(3); // Overwrites 1
        assert!(buffer.is_full());

        assert_eq!(buffer.pop(), Some(2));
        assert!(!buffer.is_full());
    }

    #[test]
    fn test_ring_buffer_peek() {
        let mut buffer: RingBuffer<i32, 4> = RingBuffer::new();

        assert_eq!(buffer.peek(), None);

        buffer.push(42);
        assert_eq!(buffer.peek(), Some(42));
        assert_eq!(buffer.len(), 1); // Peek doesn't remove

        assert_eq!(buffer.pop(), Some(42));
        assert_eq!(buffer.peek(), None);
    }

    #[test]
    fn test_ring_buffer_clear() {
        let mut buffer: RingBuffer<i32, 4> = RingBuffer::new();

        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        assert_eq!(buffer.len(), 3);
        buffer.clear();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_ring_buffer_continuous_operation() {
        let mut buffer: RingBuffer<i32, 3> = RingBuffer::new();

        for i in 0..10 {
            buffer.push(i);
            if i >= 2 {
                // After pushing 3 items, pop one
                assert_eq!(buffer.pop(), Some(i - 2));
            }
        }
    }

    #[test]
    fn test_ring_buffer_display() {
        extern crate std;
        use std::format;

        let mut buffer: RingBuffer<i32, 4> = RingBuffer::new();
        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        let s = format!("{}", buffer);
        assert_eq!(s, "[1, 2, 3]");
    }
}
