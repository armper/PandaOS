//! Simple disk-backed filesystem (read-only)
//!
//! This module implements a minimal custom filesystem for reading files
//! from disk. The on-disk layout is simple and deterministic:
//!
//! - Superblock (sector 0): Magic number, version, root inode
//! - Inode table (sectors 1-N): Array of inodes
//! - Data blocks (sectors N+1...): File and directory data
//!
//! ## On-Disk Layout
//!
//! ### Superblock (512 bytes)
//! - Magic: 0x50414E44 ("PAND")
//! - Version: 1
//! - Root inode number
//! - Total inodes
//! - First data block
//!
//! ### Inode Structure
//! - Inode number
//! - File type (file=1, dir=2)
//! - Size in bytes
//! - Direct block pointers (up to 12)
//!
//! ### Directory Entry
//! - Inode number
//! - Name length
//! - Name (null-terminated)

use alloc::string::String;
use alloc::vec::Vec;
use panda_hal::block::{BlockDevice, SECTOR_SIZE};

/// Filesystem magic number: "PAND"
const FS_MAGIC: u32 = 0x50414E44;

/// Filesystem version
const FS_VERSION: u32 = 1;

/// Maximum direct block pointers per inode
const MAX_DIRECT_BLOCKS: usize = 28;

/// Maximum filename length
const MAX_FILENAME_LEN: usize = 255;

/// Superblock structure (fits in one sector)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct Superblock {
    magic: u32,
    version: u32,
    root_inode: u32,
    total_inodes: u32,
    first_data_block: u32,
    padding: [u8; 496], // Pad to 512 bytes
}

/// Inode types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum InodeType {
    File = 1,
    Directory = 2,
}

impl InodeType {
    fn from_u32(val: u32) -> Option<Self> {
        match val {
            1 => Some(InodeType::File),
            2 => Some(InodeType::Directory),
            _ => None,
        }
    }
}

/// On-disk inode structure (128 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct Inode {
    inode_num: u32,           // 4 bytes
    file_type: u32,           // 4 bytes
    size: u64,                // 8 bytes
    direct_blocks: [u32; 28], // 112 bytes
}

/// Directory entry header (on-disk)
///
/// Layout is:
/// - inode_num: u32 (little-endian)
/// - name_len: u8
/// - name bytes (name_len)
/// - padding to 4-byte boundary
///
/// Note: We parse from raw bytes instead of a packed struct to avoid unaligned
/// field loads.
const DIRENT_HEADER_SIZE: usize = 5;

/// In-memory representation of a file
#[derive(Clone, Debug)]
pub struct DiskFile {
    pub inode: u32,
    pub file_type: InodeType,
    pub size: u64,
    pub blocks: Vec<u32>,
}

/// In-memory representation of a directory entry
#[derive(Clone, Debug)]
pub struct DiskDirEntry {
    pub inode: u32,
    pub name: String,
}

/// Disk filesystem reader
pub struct DiskFs<D: BlockDevice> {
    device: D,
    root_inode: u32,
    total_inodes: u32,
    first_data_block: u32,
}

impl<D: BlockDevice> DiskFs<D> {
    /// Create a new disk filesystem from a block device
    ///
    /// # Errors
    /// Returns an error if the superblock is invalid or cannot be read
    pub fn new(mut device: D) -> Result<Self, DiskFsError> {
        // Read superblock
        let mut sector = [0u8; SECTOR_SIZE];
        device.read_sector(0, &mut sector).map_err(|_| DiskFsError::IoError)?;

        // Parse from raw bytes to avoid unaligned loads from packed structs.
        let magic = u32::from_le_bytes([sector[0], sector[1], sector[2], sector[3]]);
        let version = u32::from_le_bytes([sector[4], sector[5], sector[6], sector[7]]);
        let root_inode = u32::from_le_bytes([sector[8], sector[9], sector[10], sector[11]]);
        let total_inodes = u32::from_le_bytes([sector[12], sector[13], sector[14], sector[15]]);
        let first_data_block = u32::from_le_bytes([sector[16], sector[17], sector[18], sector[19]]);

        // Validate magic and version
        if magic != FS_MAGIC {
            return Err(DiskFsError::InvalidMagic);
        }
        if version != FS_VERSION {
            return Err(DiskFsError::InvalidVersion);
        }

        Ok(Self { device, root_inode, total_inodes, first_data_block })
    }

    /// Read an inode from disk
    fn read_inode(&mut self, inode_num: u32) -> Result<DiskFile, DiskFsError> {
        if inode_num >= self.total_inodes {
            return Err(DiskFsError::InvalidInode);
        }

        // Inodes start at sector 1
        // Each sector holds 8 inodes (512 / 64 = 8)
        let inodes_per_sector = SECTOR_SIZE / core::mem::size_of::<Inode>();
        let sector_num = 1 + (inode_num as usize / inodes_per_sector) as u64;
        let inode_offset = (inode_num as usize % inodes_per_sector) * core::mem::size_of::<Inode>();

        let mut sector = [0u8; SECTOR_SIZE];
        self.device.read_sector(sector_num, &mut sector).map_err(|_| DiskFsError::IoError)?;

        // Parse inode fields from raw bytes (little-endian) to avoid unaligned loads.
        let base = inode_offset;
        let inode_num_on_disk = u32::from_le_bytes([
            sector[base],
            sector[base + 1],
            sector[base + 2],
            sector[base + 3],
        ]);
        let file_type_raw = u32::from_le_bytes([
            sector[base + 4],
            sector[base + 5],
            sector[base + 6],
            sector[base + 7],
        ]);
        let size = u64::from_le_bytes([
            sector[base + 8],
            sector[base + 9],
            sector[base + 10],
            sector[base + 11],
            sector[base + 12],
            sector[base + 13],
            sector[base + 14],
            sector[base + 15],
        ]);

        let file_type = InodeType::from_u32(file_type_raw).ok_or(DiskFsError::InvalidInodeType)?;

        let mut blocks = Vec::new();
        let direct_blocks_base = base + 16;
        for i in 0..MAX_DIRECT_BLOCKS {
            let off = direct_blocks_base + i * 4;
            let block = u32::from_le_bytes([
                sector[off],
                sector[off + 1],
                sector[off + 2],
                sector[off + 3],
            ]);
            if block != 0 {
                blocks.push(block);
            }
        }

        Ok(DiskFile { inode: inode_num_on_disk, file_type, size, blocks })
    }

    /// Read directory entries from a directory inode
    pub fn read_dir(&mut self, inode_num: u32) -> Result<Vec<DiskDirEntry>, DiskFsError> {
        let file = self.read_inode(inode_num)?;

        if file.file_type != InodeType::Directory {
            return Err(DiskFsError::NotADirectory);
        }

        let mut entries = Vec::new();
        let mut buffer = Vec::new();

        // Read all data blocks for this directory
        for &block_num in &file.blocks {
            let mut sector = [0u8; SECTOR_SIZE];
            self.device
                .read_sector(block_num as u64, &mut sector)
                .map_err(|_| DiskFsError::IoError)?;
            buffer.extend_from_slice(&sector);
        }

        // Parse directory entries
        let mut offset = 0;
        let limit = (file.size as usize).min(buffer.len());

        while offset < limit {
            if offset + DIRENT_HEADER_SIZE > limit {
                break;
            }

            let inode_num = u32::from_le_bytes([
                buffer[offset],
                buffer[offset + 1],
                buffer[offset + 2],
                buffer[offset + 3],
            ]);
            let name_len = buffer[offset + 4] as usize;
            offset += DIRENT_HEADER_SIZE;

            if name_len == 0 || name_len > MAX_FILENAME_LEN || offset + name_len > limit {
                break;
            }

            let name_bytes = &buffer[offset..offset + name_len];
            let name = String::from_utf8_lossy(name_bytes).into_owned();
            entries.push(DiskDirEntry { inode: inode_num, name });

            offset += name_len;
            // Align to 4-byte boundary
            offset = (offset + 3) & !3;
        }

        Ok(entries)
    }

    /// Lookup a file by name in a directory
    pub fn lookup(&mut self, dir_inode: u32, name: &str) -> Result<u32, DiskFsError> {
        let entries = self.read_dir(dir_inode)?;

        for entry in entries {
            if entry.name == name {
                return Ok(entry.inode);
            }
        }

        Err(DiskFsError::NotFound)
    }

    /// Resolve a path from root
    pub fn resolve_path(&mut self, path: &str) -> Result<u32, DiskFsError> {
        if path.is_empty() || !path.starts_with('/') {
            return Err(DiskFsError::InvalidPath);
        }

        if path == "/" {
            return Ok(self.root_inode);
        }

        let mut current_inode = self.root_inode;
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        for component in components {
            current_inode = self.lookup(current_inode, component)?;
        }

        Ok(current_inode)
    }

    /// Read file data
    pub fn read_file(
        &mut self,
        inode_num: u32,
        offset: usize,
        buffer: &mut [u8],
    ) -> Result<usize, DiskFsError> {
        let file = self.read_inode(inode_num)?;

        if file.file_type != InodeType::File {
            return Err(DiskFsError::NotAFile);
        }

        if offset >= file.size as usize {
            return Ok(0); // EOF
        }

        let available = (file.size as usize - offset).min(buffer.len());
        let mut bytes_read = 0;

        // Calculate which block to start reading from
        let start_block_idx = offset / SECTOR_SIZE;
        let start_block_offset = offset % SECTOR_SIZE;

        for (i, &block_num) in file.blocks.iter().enumerate().skip(start_block_idx) {
            if bytes_read >= available {
                break;
            }

            let mut sector = [0u8; SECTOR_SIZE];
            self.device
                .read_sector(block_num as u64, &mut sector)
                .map_err(|_| DiskFsError::IoError)?;

            let block_offset = if i == start_block_idx { start_block_offset } else { 0 };
            let to_read = (SECTOR_SIZE - block_offset).min(available - bytes_read);

            buffer[bytes_read..bytes_read + to_read]
                .copy_from_slice(&sector[block_offset..block_offset + to_read]);

            bytes_read += to_read;
        }

        Ok(bytes_read)
    }

    /// Get file metadata
    pub fn stat(&mut self, inode_num: u32) -> Result<DiskFile, DiskFsError> {
        self.read_inode(inode_num)
    }

    /// Get root inode number
    pub fn root_inode(&self) -> u32 {
        self.root_inode
    }
}

/// Disk filesystem errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskFsError {
    IoError,
    InvalidMagic,
    InvalidVersion,
    InvalidInode,
    InvalidInodeType,
    InvalidPath,
    NotFound,
    NotADirectory,
    NotAFile,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock block device for testing
    struct MockDevice {
        sectors: Vec<[u8; SECTOR_SIZE]>,
    }

    impl MockDevice {
        fn new(num_sectors: usize) -> Self {
            Self { sectors: vec![[0u8; SECTOR_SIZE]; num_sectors] }
        }
    }

    impl BlockDevice for MockDevice {
        fn read_sector(
            &mut self,
            sector: u64,
            buffer: &mut [u8; SECTOR_SIZE],
        ) -> Result<(), BlockError> {
            if sector as usize >= self.sectors.len() {
                return Err(BlockError::InvalidSector);
            }
            buffer.copy_from_slice(&self.sectors[sector as usize]);
            Ok(())
        }
    }

    #[test]
    fn test_superblock_size() {
        assert_eq!(core::mem::size_of::<Superblock>(), SECTOR_SIZE);
    }

    #[test]
    fn test_inode_size() {
        assert_eq!(core::mem::size_of::<Inode>(), 64);
    }

    #[test]
    fn test_invalid_magic() {
        let device = MockDevice::new(1);
        let result = DiskFs::new(device);
        assert_eq!(result.unwrap_err(), DiskFsError::InvalidMagic);
    }
}
