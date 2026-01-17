//! Block device abstraction for disk I/O
//!
//! This module provides a simple block device interface for reading and writing
//! sectors to disk devices.

/// Standard sector size for ATA/IDE devices
pub const SECTOR_SIZE: usize = 512;

/// Block device trait for reading and writing disk sectors
pub trait BlockDevice {
    /// Read a single sector (512 bytes) from the device
    ///
    /// # Arguments
    /// * `sector` - The sector number to read (0-indexed)
    /// * `buffer` - A 512-byte buffer to read into
    ///
    /// # Errors
    /// Returns an error if the read fails
    fn read_sector(
        &mut self,
        sector: u64,
        buffer: &mut [u8; SECTOR_SIZE],
    ) -> Result<(), BlockError>;

    /// Write a single sector (512 bytes) to the device
    ///
    /// # Arguments
    /// * `sector` - The sector number to write (0-indexed)
    /// * `buffer` - A 512-byte buffer to write from
    ///
    /// # Errors
    /// Returns an error if the write fails
    fn write_sector(&mut self, sector: u64, buffer: &[u8; SECTOR_SIZE]) -> Result<(), BlockError>;

    /// Read multiple contiguous sectors
    ///
    /// # Arguments
    /// * `start_sector` - First sector to read
    /// * `count` - Number of sectors to read
    /// * `buffer` - Buffer to read into (must be at least `count * SECTOR_SIZE` bytes)
    ///
    /// # Errors
    /// Returns an error if the read fails or buffer is too small
    fn read_sectors(
        &mut self,
        start_sector: u64,
        count: usize,
        buffer: &mut [u8],
    ) -> Result<(), BlockError> {
        if buffer.len() < count * SECTOR_SIZE {
            return Err(BlockError::BufferTooSmall);
        }

        let mut sector_buf = [0u8; SECTOR_SIZE];
        for i in 0..count {
            self.read_sector(start_sector + i as u64, &mut sector_buf)?;
            let offset = i * SECTOR_SIZE;
            buffer[offset..offset + SECTOR_SIZE].copy_from_slice(&sector_buf);
        }

        Ok(())
    }

    /// Write multiple contiguous sectors
    ///
    /// # Arguments
    /// * `start_sector` - First sector to write
    /// * `count` - Number of sectors to write
    /// * `buffer` - Buffer to write from (must be at least `count * SECTOR_SIZE` bytes)
    ///
    /// # Errors
    /// Returns an error if the write fails or buffer is too small
    fn write_sectors(
        &mut self,
        start_sector: u64,
        count: usize,
        buffer: &[u8],
    ) -> Result<(), BlockError> {
        if buffer.len() < count * SECTOR_SIZE {
            return Err(BlockError::BufferTooSmall);
        }

        let mut sector_buf = [0u8; SECTOR_SIZE];
        for i in 0..count {
            let offset = i * SECTOR_SIZE;
            sector_buf.copy_from_slice(&buffer[offset..offset + SECTOR_SIZE]);
            self.write_sector(start_sector + i as u64, &sector_buf)?;
        }

        Ok(())
    }
}

/// Errors that can occur during block device operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError {
    /// Hardware I/O error
    IoError,
    /// Invalid sector number
    InvalidSector,
    /// Buffer too small for operation
    BufferTooSmall,
    /// Device not ready
    NotReady,
}
