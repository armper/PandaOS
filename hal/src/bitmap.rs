//! Bitmap data structure for tracking allocations
//!
//! Pure logic for bitmap-based allocation tracking, testable on host.

/// A simple bitmap for tracking allocations
#[derive(Debug)]
pub struct Bitmap {
    data: &'static mut [u8],
    size: usize,
}

impl Bitmap {
    /// Create a new bitmap from a mutable byte slice
    ///
    /// # Arguments
    ///
    /// * `data` - Byte slice to use for bitmap storage
    /// * `size` - Number of bits to track
    pub fn new(data: &'static mut [u8], size: usize) -> Self {
        let required_bytes = size.div_ceil(8);
        assert!(
            data.len() >= required_bytes,
            "Bitmap data too small: {} bytes available, {} required",
            data.len(),
            required_bytes
        );

        // Clear all bits initially
        for byte in &mut data[..required_bytes] {
            *byte = 0;
        }

        Self { data, size }
    }

    /// Set a bit in the bitmap
    pub fn set(&mut self, index: usize) {
        assert!(index < self.size, "Bitmap index out of bounds");
        let byte_index = index / 8;
        let bit_index = index % 8;
        self.data[byte_index] |= 1 << bit_index;
    }

    /// Clear a bit in the bitmap
    pub fn clear(&mut self, index: usize) {
        assert!(index < self.size, "Bitmap index out of bounds");
        let byte_index = index / 8;
        let bit_index = index % 8;
        self.data[byte_index] &= !(1 << bit_index);
    }

    /// Test if a bit is set
    pub fn is_set(&self, index: usize) -> bool {
        assert!(index < self.size, "Bitmap index out of bounds");
        let byte_index = index / 8;
        let bit_index = index % 8;
        (self.data[byte_index] & (1 << bit_index)) != 0
    }

    /// Find the first clear bit and set it
    ///
    /// Returns the index of the bit, or None if all bits are set
    pub fn find_and_set(&mut self) -> Option<usize> {
        for i in 0..self.size {
            if !self.is_set(i) {
                self.set(i);
                return Some(i);
            }
        }
        None
    }

    /// Count the number of set bits
    pub fn count_set(&self) -> usize {
        let mut count = 0;
        for i in 0..self.size {
            if self.is_set(i) {
                count += 1;
            }
        }
        count
    }

    /// Count the number of clear bits
    pub fn count_clear(&self) -> usize {
        self.size - self.count_set()
    }

    /// Get the total size of the bitmap (in bits)
    pub fn size(&self) -> usize {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::vec;

    fn create_test_bitmap(size: usize) -> Bitmap {
        let required_bytes = size.div_ceil(8);
        let mut data = vec![0u8; required_bytes];

        // SAFETY: We're creating a test bitmap with a leaked Vec
        // This is acceptable in tests
        let static_data: &'static mut [u8] =
            unsafe { core::slice::from_raw_parts_mut(data.as_mut_ptr(), data.len()) };
        core::mem::forget(data);

        Bitmap::new(static_data, size)
    }

    #[test]
    fn test_bitmap_creation() {
        let bitmap = create_test_bitmap(64);
        assert_eq!(bitmap.size(), 64);
        assert_eq!(bitmap.count_set(), 0);
        assert_eq!(bitmap.count_clear(), 64);
    }

    #[test]
    fn test_bitmap_set_clear() {
        let mut bitmap = create_test_bitmap(32);

        assert!(!bitmap.is_set(5));
        bitmap.set(5);
        assert!(bitmap.is_set(5));
        bitmap.clear(5);
        assert!(!bitmap.is_set(5));
    }

    #[test]
    fn test_bitmap_multiple_bits() {
        let mut bitmap = create_test_bitmap(16);

        bitmap.set(0);
        bitmap.set(7);
        bitmap.set(15);

        assert!(bitmap.is_set(0));
        assert!(bitmap.is_set(7));
        assert!(bitmap.is_set(15));
        assert!(!bitmap.is_set(1));
        assert!(!bitmap.is_set(8));

        assert_eq!(bitmap.count_set(), 3);
        assert_eq!(bitmap.count_clear(), 13);
    }

    #[test]
    fn test_bitmap_find_and_set() {
        let mut bitmap = create_test_bitmap(8);

        assert_eq!(bitmap.find_and_set(), Some(0));
        assert_eq!(bitmap.find_and_set(), Some(1));
        assert_eq!(bitmap.find_and_set(), Some(2));

        assert_eq!(bitmap.count_set(), 3);
    }

    #[test]
    fn test_bitmap_exhaustion() {
        let mut bitmap = create_test_bitmap(4);

        for i in 0..4 {
            assert_eq!(bitmap.find_and_set(), Some(i));
        }

        assert_eq!(bitmap.find_and_set(), None);
        assert_eq!(bitmap.count_set(), 4);
        assert_eq!(bitmap.count_clear(), 0);
    }

    #[test]
    fn test_bitmap_across_bytes() {
        let mut bitmap = create_test_bitmap(24);

        bitmap.set(7); // Last bit of first byte
        bitmap.set(8); // First bit of second byte
        bitmap.set(16); // First bit of third byte

        assert!(bitmap.is_set(7));
        assert!(bitmap.is_set(8));
        assert!(bitmap.is_set(16));
        assert_eq!(bitmap.count_set(), 3);
    }

    #[test]
    #[should_panic(expected = "Bitmap index out of bounds")]
    fn test_bitmap_out_of_bounds() {
        let mut bitmap = create_test_bitmap(8);
        bitmap.set(8); // Should panic
    }
}
