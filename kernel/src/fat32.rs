//! FAT32 filesystem implementation (read-only)
//!
//! This module implements a minimal FAT32 filesystem driver for reading files
//! and directories from a FAT32-formatted disk image.
//!
//! ## FAT32 Layout
//!
//! - Boot sector (sector 0): BPB (BIOS Parameter Block) and EBPB (Extended BPB)
//! - Reserved sectors: Usually FSInfo and backup boot sector
//! - FAT tables: Two copies for redundancy
//! - Root directory: Starts at root cluster (usually cluster 2)
//! - Data region: File and directory data
//!
//! ## Features
//!
//! - Read-only access to files and directories
//! - Support for short 8.3 filenames
//! - Support for Long File Names (LFN)
//! - Cluster chain traversal
//! - Directory listing
//! - File reading with proper cluster handling

use alloc::string::String;
use alloc::vec::Vec;
use panda_hal::block::{BlockDevice, BlockError, SECTOR_SIZE};

/// FAT32 boot sector signature
const FAT32_SIGNATURE: u16 = 0xAA55;

/// FAT32 filesystem type string (in EBPB)
const FAT32_FSTYPE: &[u8] = b"FAT32   ";

/// End of cluster chain marker
const EOC_MARKER: u32 = 0x0FFFFFF8;

/// Bad cluster marker
const BAD_CLUSTER: u32 = 0x0FFFFFF7;

/// Maximum cluster value (28-bit)
const MAX_CLUSTER: u32 = 0x0FFFFFFF;

/// Directory entry size (32 bytes)
const DIRENT_SIZE: usize = 32;

/// Attribute: Read-only
const ATTR_READ_ONLY: u8 = 0x01;
/// Attribute: Hidden
const ATTR_HIDDEN: u8 = 0x02;
/// Attribute: System
const ATTR_SYSTEM: u8 = 0x04;
/// Attribute: Volume ID
const ATTR_VOLUME_ID: u8 = 0x08;
/// Attribute: Directory
const ATTR_DIRECTORY: u8 = 0x10;
/// Attribute: Archive
const ATTR_ARCHIVE: u8 = 0x20;
/// Attribute: Long File Name (combination of read-only, hidden, system, volume)
const ATTR_LONG_NAME: u8 = ATTR_READ_ONLY | ATTR_HIDDEN | ATTR_SYSTEM | ATTR_VOLUME_ID;

/// BIOS Parameter Block (BPB) for FAT32
#[derive(Debug, Clone, Copy)]
struct Bpb {
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    total_sectors: u32,
    sectors_per_fat: u32,
    root_cluster: u32,
    fat_start_sector: u32,
    data_start_sector: u32,
}

impl Bpb {
    /// Parse BPB from boot sector
    fn parse(boot_sector: &[u8; SECTOR_SIZE]) -> Result<Self, Fat32Error> {
        // Check boot signature
        let signature = u16::from_le_bytes([boot_sector[510], boot_sector[511]]);
        if signature != FAT32_SIGNATURE {
            return Err(Fat32Error::InvalidSignature);
        }

        // Parse BPB fields
        let bytes_per_sector = u16::from_le_bytes([boot_sector[11], boot_sector[12]]);
        let sectors_per_cluster = boot_sector[13];
        let reserved_sectors = u16::from_le_bytes([boot_sector[14], boot_sector[15]]);
        let num_fats = boot_sector[16];

        // Total sectors (32-bit for FAT32)
        let total_sectors = u32::from_le_bytes([
            boot_sector[32],
            boot_sector[33],
            boot_sector[34],
            boot_sector[35],
        ]);

        // Sectors per FAT (32-bit for FAT32)
        let sectors_per_fat = u32::from_le_bytes([
            boot_sector[36],
            boot_sector[37],
            boot_sector[38],
            boot_sector[39],
        ]);

        // Root directory cluster
        let root_cluster = u32::from_le_bytes([
            boot_sector[44],
            boot_sector[45],
            boot_sector[46],
            boot_sector[47],
        ]);

        // Validate FAT32 filesystem type (optional check)
        // The filesystem type string is at offset 82 in the extended boot record
        let fstype = &boot_sector[82..90];
        if fstype != FAT32_FSTYPE {
            // Some FAT32 volumes may not have this string, so we don't fail here
            // but we can log a warning in a real implementation
        }

        // Calculate FAT and data region start sectors
        let fat_start_sector = reserved_sectors as u32;
        let data_start_sector = fat_start_sector + (num_fats as u32 * sectors_per_fat);

        // Validate parameters
        if bytes_per_sector != SECTOR_SIZE as u16 {
            return Err(Fat32Error::UnsupportedSectorSize);
        }
        if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
            return Err(Fat32Error::InvalidBpb);
        }
        if num_fats == 0 || reserved_sectors == 0 {
            return Err(Fat32Error::InvalidBpb);
        }

        Ok(Bpb {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            total_sectors,
            sectors_per_fat,
            root_cluster,
            fat_start_sector,
            data_start_sector,
        })
    }

    /// Get the first sector of a cluster
    fn cluster_to_sector(&self, cluster: u32) -> u64 {
        // Clusters start at 2, so subtract 2 from cluster number
        let cluster_offset = if cluster >= 2 { cluster - 2 } else { 0 };
        self.data_start_sector as u64 + (cluster_offset as u64 * self.sectors_per_cluster as u64)
    }

    /// Get the number of bytes per cluster
    fn bytes_per_cluster(&self) -> usize {
        self.bytes_per_sector as usize * self.sectors_per_cluster as usize
    }
}

/// Directory entry (short 8.3 format)
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub attributes: u8,
    pub first_cluster: u32,
    pub file_size: u32,
}

impl DirEntry {
    /// Parse a directory entry from 32 bytes
    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < DIRENT_SIZE {
            return None;
        }

        // Check if entry is free or deleted
        let first_byte = data[0];
        if first_byte == 0x00 || first_byte == 0xE5 {
            return None;
        }

        // Get attributes
        let attributes = data[11];

        // Skip volume ID entries
        if (attributes & ATTR_VOLUME_ID) != 0 && (attributes & ATTR_DIRECTORY) == 0 {
            return None;
        }

        // Parse name (8.3 format)
        let name_bytes = &data[0..11];
        let name = Self::parse_short_name(name_bytes);

        // Get first cluster (high 16 bits at offset 20, low 16 bits at offset 26)
        let first_cluster_high = u16::from_le_bytes([data[20], data[21]]) as u32;
        let first_cluster_low = u16::from_le_bytes([data[26], data[27]]) as u32;
        let first_cluster = (first_cluster_high << 16) | first_cluster_low;

        // Get file size (32 bits at offset 28)
        let file_size = u32::from_le_bytes([data[28], data[29], data[30], data[31]]);

        Some(DirEntry { name, attributes, first_cluster, file_size })
    }

    /// Parse 8.3 filename into readable string
    fn parse_short_name(name_bytes: &[u8]) -> String {
        let name_part = core::str::from_utf8(&name_bytes[0..8]).unwrap_or("????????").trim_end();
        let ext_part = core::str::from_utf8(&name_bytes[8..11]).unwrap_or("???").trim_end();

        if ext_part.is_empty() {
            name_part.to_lowercase()
        } else {
            alloc::format!("{}.{}", name_part.to_lowercase(), ext_part.to_lowercase())
        }
    }

    /// Check if this is a directory
    pub fn is_directory(&self) -> bool {
        (self.attributes & ATTR_DIRECTORY) != 0
    }
}

/// Long File Name (LFN) entry
#[derive(Debug)]
struct LfnEntry {
    sequence: u8,
    name_part: String,
}

impl LfnEntry {
    /// Parse a LFN entry from 32 bytes
    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < DIRENT_SIZE {
            return None;
        }

        let attributes = data[11];
        if attributes != ATTR_LONG_NAME {
            return None;
        }

        let sequence = data[0];
        if sequence == 0x00 || sequence == 0xE5 {
            return None;
        }

        // Extract name parts (5 + 6 + 2 = 13 characters)
        let mut name_chars = Vec::new();

        // Characters 1-5 (offset 1-10, UTF-16LE)
        for i in 0..5 {
            let offset = 1 + i * 2;
            let ch = u16::from_le_bytes([data[offset], data[offset + 1]]);
            if ch != 0 && ch != 0xFFFF {
                name_chars.push(ch);
            }
        }

        // Characters 6-11 (offset 14-25, UTF-16LE)
        for i in 0..6 {
            let offset = 14 + i * 2;
            let ch = u16::from_le_bytes([data[offset], data[offset + 1]]);
            if ch != 0 && ch != 0xFFFF {
                name_chars.push(ch);
            }
        }

        // Characters 12-13 (offset 28-31, UTF-16LE)
        for i in 0..2 {
            let offset = 28 + i * 2;
            let ch = u16::from_le_bytes([data[offset], data[offset + 1]]);
            if ch != 0 && ch != 0xFFFF {
                name_chars.push(ch);
            }
        }

        let name_part = String::from_utf16_lossy(&name_chars);
        Some(LfnEntry { sequence, name_part })
    }
}

/// FAT32 filesystem
pub struct Fat32<D: BlockDevice> {
    device: D,
    bpb: Bpb,
}

impl<D: BlockDevice> Fat32<D> {
    /// Create a new FAT32 filesystem from a block device
    pub fn new(mut device: D) -> Result<Self, Fat32Error> {
        // Read boot sector
        let mut boot_sector = [0u8; SECTOR_SIZE];
        device.read_sector(0, &mut boot_sector).map_err(|_| Fat32Error::IoError)?;

        // Parse BPB
        let bpb = Bpb::parse(&boot_sector)?;

        Ok(Self { device, bpb })
    }

    /// Read a FAT entry (next cluster in chain)
    fn read_fat_entry(&mut self, cluster: u32) -> Result<u32, Fat32Error> {
        if cluster < 2 || cluster >= MAX_CLUSTER {
            return Err(Fat32Error::InvalidCluster);
        }

        // Calculate FAT offset
        let fat_offset = cluster * 4; // 4 bytes per FAT32 entry
        let fat_sector =
            self.bpb.fat_start_sector as u64 + (fat_offset / SECTOR_SIZE as u32) as u64;
        let entry_offset = (fat_offset % SECTOR_SIZE as u32) as usize;

        // Read FAT sector
        let mut sector = [0u8; SECTOR_SIZE];
        self.device.read_sector(fat_sector, &mut sector).map_err(|_| Fat32Error::IoError)?;

        // Extract 28-bit FAT entry (mask out upper 4 bits)
        let entry = u32::from_le_bytes([
            sector[entry_offset],
            sector[entry_offset + 1],
            sector[entry_offset + 2],
            sector[entry_offset + 3],
        ]) & 0x0FFFFFFF;

        Ok(entry)
    }

    /// Get the cluster chain for a file/directory
    fn get_cluster_chain(&mut self, start_cluster: u32) -> Result<Vec<u32>, Fat32Error> {
        if start_cluster == 0 {
            return Ok(Vec::new());
        }

        let mut chain = Vec::new();
        let mut current_cluster = start_cluster;

        // Follow the chain (with a safety limit)
        const MAX_CHAIN_LENGTH: usize = 65536; // Prevent infinite loops
        while current_cluster < EOC_MARKER && chain.len() < MAX_CHAIN_LENGTH {
            if current_cluster == BAD_CLUSTER {
                return Err(Fat32Error::BadCluster);
            }

            chain.push(current_cluster);
            current_cluster = self.read_fat_entry(current_cluster)?;
        }

        Ok(chain)
    }

    /// Read a cluster's data
    fn read_cluster(&mut self, cluster: u32, buffer: &mut Vec<u8>) -> Result<(), Fat32Error> {
        let start_sector = self.bpb.cluster_to_sector(cluster);
        let bytes_per_cluster = self.bpb.bytes_per_cluster();

        buffer.reserve(bytes_per_cluster);
        let start_len = buffer.len();

        for i in 0..self.bpb.sectors_per_cluster {
            let mut sector = [0u8; SECTOR_SIZE];
            self.device
                .read_sector(start_sector + i as u64, &mut sector)
                .map_err(|_| Fat32Error::IoError)?;
            buffer.extend_from_slice(&sector);
        }

        // Verify we read the expected amount
        debug_assert_eq!(buffer.len() - start_len, bytes_per_cluster);

        Ok(())
    }

    /// Read directory entries from a cluster chain
    pub fn read_directory(&mut self, cluster: u32) -> Result<Vec<DirEntry>, Fat32Error> {
        let chain = self.get_cluster_chain(cluster)?;
        let mut buffer = Vec::new();

        // Read all clusters
        for &cluster_num in &chain {
            self.read_cluster(cluster_num, &mut buffer)?;
        }

        // Parse directory entries
        let mut entries = Vec::new();
        let mut lfn_entries: Vec<LfnEntry> = Vec::new();

        let mut offset = 0;
        while offset + DIRENT_SIZE <= buffer.len() {
            let entry_data = &buffer[offset..offset + DIRENT_SIZE];

            // Check if this is the end of directory
            if entry_data[0] == 0x00 {
                break;
            }

            // Try to parse as LFN entry first
            if let Some(lfn) = LfnEntry::parse(entry_data) {
                lfn_entries.push(lfn);
            } else if let Some(mut dir_entry) = DirEntry::parse(entry_data) {
                // Reconstruct long filename if we have LFN entries
                if !lfn_entries.is_empty() {
                    // LFN entries are in reverse order
                    lfn_entries.reverse();
                    let long_name: String =
                        lfn_entries.iter().map(|e| e.name_part.as_str()).collect();
                    dir_entry.name = long_name;
                    lfn_entries.clear();
                }

                // Skip "." and ".." entries for simplicity
                if dir_entry.name != "." && dir_entry.name != ".." {
                    entries.push(dir_entry);
                }
            } else {
                // Clear LFN entries if we encounter an invalid entry
                lfn_entries.clear();
            }

            offset += DIRENT_SIZE;
        }

        Ok(entries)
    }

    /// Lookup a file/directory by name in a directory
    pub fn lookup(&mut self, dir_cluster: u32, name: &str) -> Result<DirEntry, Fat32Error> {
        let entries = self.read_directory(dir_cluster)?;

        for entry in entries {
            if entry.name.eq_ignore_ascii_case(name) {
                return Ok(entry);
            }
        }

        Err(Fat32Error::NotFound)
    }

    /// Resolve a path from root
    pub fn resolve_path(&mut self, path: &str) -> Result<DirEntry, Fat32Error> {
        if path.is_empty() || !path.starts_with('/') {
            return Err(Fat32Error::InvalidPath);
        }

        // Root directory
        if path == "/" {
            return Ok(DirEntry {
                name: String::from("/"),
                attributes: ATTR_DIRECTORY,
                first_cluster: self.bpb.root_cluster,
                file_size: 0,
            });
        }

        let mut current_cluster = self.bpb.root_cluster;
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        let mut current_entry = None;
        for component in components {
            let entry = self.lookup(current_cluster, component)?;
            if !entry.is_directory() && current_entry.is_some() {
                return Err(Fat32Error::NotADirectory);
            }
            current_cluster = entry.first_cluster;
            current_entry = Some(entry);
        }

        current_entry.ok_or(Fat32Error::NotFound)
    }

    /// Read file data
    pub fn read_file(
        &mut self,
        start_cluster: u32,
        file_size: u32,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<usize, Fat32Error> {
        if start_cluster == 0 || file_size == 0 {
            return Ok(0);
        }

        if offset >= file_size as u64 {
            return Ok(0);
        }

        // Get cluster chain
        let chain = self.get_cluster_chain(start_cluster)?;
        if chain.is_empty() {
            return Ok(0);
        }

        let bytes_per_cluster = self.bpb.bytes_per_cluster();
        let max_read = (file_size as u64 - offset).min(buffer.len() as u64) as usize;
        let mut bytes_read = 0;

        // Calculate starting cluster and offset within that cluster
        let start_cluster_idx = (offset / bytes_per_cluster as u64) as usize;
        let cluster_offset = (offset % bytes_per_cluster as u64) as usize;

        for (i, &cluster_num) in chain.iter().enumerate().skip(start_cluster_idx) {
            if bytes_read >= max_read {
                break;
            }

            // Read cluster
            let mut cluster_data = Vec::new();
            self.read_cluster(cluster_num, &mut cluster_data)?;

            // Determine how much to copy from this cluster
            let copy_start = if i == start_cluster_idx { cluster_offset } else { 0 };
            let copy_end = cluster_data.len().min(copy_start + (max_read - bytes_read));
            let copy_len = copy_end - copy_start;

            if copy_len > 0 {
                buffer[bytes_read..bytes_read + copy_len]
                    .copy_from_slice(&cluster_data[copy_start..copy_end]);
                bytes_read += copy_len;
            }
        }

        Ok(bytes_read)
    }

    /// Get root directory cluster
    pub fn root_cluster(&self) -> u32 {
        self.bpb.root_cluster
    }
}

/// FAT32 filesystem errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fat32Error {
    /// I/O error reading from device
    IoError,
    /// Invalid boot sector signature
    InvalidSignature,
    /// Invalid BPB parameters
    InvalidBpb,
    /// Unsupported sector size
    UnsupportedSectorSize,
    /// Invalid cluster number
    InvalidCluster,
    /// Bad cluster in chain
    BadCluster,
    /// File or directory not found
    NotFound,
    /// Path is not a directory
    NotADirectory,
    /// Invalid path
    InvalidPath,
}

impl From<BlockError> for Fat32Error {
    fn from(_: BlockError) -> Self {
        Fat32Error::IoError
    }
}
