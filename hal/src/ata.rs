//! ATA/IDE driver for primary master disk (PIO mode)
//!
//! This is a minimal ATA driver supporting only the primary master disk
//! in PIO (Programmed I/O) mode. Sufficient for QEMU testing.

#[cfg(feature = "hardware")]
use crate::block::{BlockDevice, BlockError, SECTOR_SIZE};

#[cfg(feature = "hardware")]
use x86_64::instructions::port::Port;

/// ATA primary bus I/O base port
const ATA_PRIMARY_IO: u16 = 0x1F0;

/// ATA port offsets from base
const ATA_DATA: u16 = 0;
const ATA_SECTOR_COUNT: u16 = 2;
const ATA_LBA_LOW: u16 = 3;
const ATA_LBA_MID: u16 = 4;
const ATA_LBA_HIGH: u16 = 5;
const ATA_DRIVE_SELECT: u16 = 6;
const ATA_COMMAND: u16 = 7;
const ATA_STATUS: u16 = 7;

/// ATA status register bits
const ATA_STATUS_BSY: u8 = 0x80; // Busy
const ATA_STATUS_DRQ: u8 = 0x08; // Data request ready
const ATA_STATUS_ERR: u8 = 0x01; // Error

/// ATA commands
const ATA_CMD_READ_SECTORS: u8 = 0x20;
const ATA_CMD_WRITE_SECTORS: u8 = 0x30;

/// Primary master ATA disk driver
#[cfg(feature = "hardware")]
pub struct AtaDisk {
    io_base: u16,
}

#[cfg(feature = "hardware")]
impl AtaDisk {
    /// Create a new ATA disk driver for the primary master disk
    ///
    /// # Safety
    /// Must only be called once, and only after proper hardware initialization.
    /// Assumes the ATA controller is present and configured.
    pub const unsafe fn new() -> Self {
        Self { io_base: ATA_PRIMARY_IO }
    }

    /// Wait for the drive to be ready (not busy)
    fn wait_not_busy(&mut self) -> Result<(), BlockError> {
        let mut status_port: Port<u8> = Port::new(self.io_base + ATA_STATUS);

        // Wait for BSY to clear (with timeout)
        for _ in 0..1000 {
            // SAFETY: Reading from ATA status port
            let status: u8 = unsafe { status_port.read() };
            if (status & ATA_STATUS_BSY) == 0 {
                return Ok(());
            }
            // Small delay
            for _ in 0..100 {
                core::hint::spin_loop();
            }
        }

        Err(BlockError::NotReady)
    }

    /// Wait for data request ready
    fn wait_drq(&mut self) -> Result<(), BlockError> {
        let mut status_port: Port<u8> = Port::new(self.io_base + ATA_STATUS);

        // Wait for DRQ to be set
        for _ in 0..1000 {
            // SAFETY: Reading from ATA status port
            let status: u8 = unsafe { status_port.read() };
            if (status & ATA_STATUS_DRQ) != 0 {
                return Ok(());
            }
            if (status & ATA_STATUS_ERR) != 0 {
                return Err(BlockError::IoError);
            }
            // Small delay
            for _ in 0..100 {
                core::hint::spin_loop();
            }
        }

        Err(BlockError::NotReady)
    }
}

#[cfg(feature = "hardware")]
impl BlockDevice for AtaDisk {
    fn read_sector(
        &mut self,
        sector: u64,
        buffer: &mut [u8; SECTOR_SIZE],
    ) -> Result<(), BlockError> {
        // Only support 28-bit LBA
        if sector >= (1 << 28) {
            return Err(BlockError::InvalidSector);
        }

        // Wait for drive to be ready
        self.wait_not_busy()?;

        // Select drive (0xE0 = master, LBA mode, bits 24-27 of LBA)
        let drive = 0xE0 | (((sector >> 24) & 0x0F) as u8);
        let mut drive_port: Port<u8> = Port::new(self.io_base + ATA_DRIVE_SELECT);
        // SAFETY: Writing to ATA drive select port
        unsafe { drive_port.write(drive) };

        // Set sector count to 1
        let mut count_port: Port<u8> = Port::new(self.io_base + ATA_SECTOR_COUNT);
        // SAFETY: Writing to ATA sector count port
        unsafe { count_port.write(1u8) };

        // Set LBA (bits 0-7, 8-15, 16-23)
        let mut lba_low: Port<u8> = Port::new(self.io_base + ATA_LBA_LOW);
        let mut lba_mid: Port<u8> = Port::new(self.io_base + ATA_LBA_MID);
        let mut lba_high: Port<u8> = Port::new(self.io_base + ATA_LBA_HIGH);

        // SAFETY: Writing LBA address to ATA ports
        unsafe {
            lba_low.write((sector & 0xFF) as u8);
            lba_mid.write(((sector >> 8) & 0xFF) as u8);
            lba_high.write(((sector >> 16) & 0xFF) as u8);
        }

        // Send read command
        let mut cmd_port: Port<u8> = Port::new(self.io_base + ATA_COMMAND);
        // SAFETY: Writing read command to ATA command port
        unsafe { cmd_port.write(ATA_CMD_READ_SECTORS) };

        // Wait for data to be ready
        self.wait_drq()?;

        // Read 512 bytes (256 words) from data port
        let mut data_port: Port<u16> = Port::new(self.io_base + ATA_DATA);
        for i in 0..256 {
            // SAFETY: Reading data from ATA data port
            let word: u16 = unsafe { data_port.read() };
            let offset = i * 2;
            buffer[offset] = (word & 0xFF) as u8;
            buffer[offset + 1] = ((word >> 8) & 0xFF) as u8;
        }

        Ok(())
    }

    fn write_sector(
        &mut self,
        sector: u64,
        buffer: &[u8; SECTOR_SIZE],
    ) -> Result<(), BlockError> {
        // Only support 28-bit LBA
        if sector >= (1 << 28) {
            return Err(BlockError::InvalidSector);
        }

        // Wait for drive to be ready
        self.wait_not_busy()?;

        // Select drive (0xE0 = master, LBA mode, bits 24-27 of LBA)
        let drive = 0xE0 | (((sector >> 24) & 0x0F) as u8);
        let mut drive_port: Port<u8> = Port::new(self.io_base + ATA_DRIVE_SELECT);
        // SAFETY: Writing to ATA drive select port
        unsafe { drive_port.write(drive) };

        // Set sector count to 1
        let mut count_port: Port<u8> = Port::new(self.io_base + ATA_SECTOR_COUNT);
        // SAFETY: Writing to ATA sector count port
        unsafe { count_port.write(1u8) };

        // Set LBA (bits 0-7, 8-15, 16-23)
        let mut lba_low: Port<u8> = Port::new(self.io_base + ATA_LBA_LOW);
        let mut lba_mid: Port<u8> = Port::new(self.io_base + ATA_LBA_MID);
        let mut lba_high: Port<u8> = Port::new(self.io_base + ATA_LBA_HIGH);

        // SAFETY: Writing LBA address to ATA ports
        unsafe {
            lba_low.write((sector & 0xFF) as u8);
            lba_mid.write(((sector >> 8) & 0xFF) as u8);
            lba_high.write(((sector >> 16) & 0xFF) as u8);
        }

        // Send write command
        let mut cmd_port: Port<u8> = Port::new(self.io_base + ATA_COMMAND);
        // SAFETY: Writing write command to ATA command port
        unsafe { cmd_port.write(ATA_CMD_WRITE_SECTORS) };

        // Wait for data request ready
        self.wait_drq()?;

        // Write 512 bytes (256 words) to data port
        let mut data_port: Port<u16> = Port::new(self.io_base + ATA_DATA);
        for i in 0..256 {
            let offset = i * 2;
            let word = (buffer[offset] as u16) | ((buffer[offset + 1] as u16) << 8);
            // SAFETY: Writing data to ATA data port
            unsafe { data_port.write(word) };
        }

        // Wait for write to complete
        self.wait_not_busy()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Note: These tests cannot run on host since they require hardware
    // Hardware-dependent tests are validated via QEMU integration tests
}
