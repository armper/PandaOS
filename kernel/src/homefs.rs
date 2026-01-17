//! Home filesystem - Persistent writable filesystem
//!
//! This module implements a simple persistent filesystem for /home.
//! See FS_ON_DISK.md for format documentation.

use panda_hal::block::{BlockDevice, BlockError, SECTOR_SIZE};

// External allocations used in readdir
extern crate alloc;

/// Filesystem magic number: "PAND"
const FS_MAGIC: u32 = 0x50414E44;

/// Filesystem version (v2 = writable)
const FS_VERSION: u32 = 2;

/// Block size (matches sector size)
const BLOCK_SIZE: usize = 512;

/// Maximum direct blocks per inode
const MAX_DIRECT_BLOCKS: usize = 8;

/// Inode size in bytes
const INODE_SIZE: usize = 64;

/// Inodes per sector
const INODES_PER_SECTOR: usize = SECTOR_SIZE / INODE_SIZE;

/// Directory entry size
const DIR_ENTRY_SIZE: usize = 256;

/// Entries per directory block
const ENTRIES_PER_BLOCK: usize = BLOCK_SIZE / DIR_ENTRY_SIZE;

/// Maximum filename length
const MAX_FILENAME_LEN: usize = 248;

/// Total filesystem size in sectors
const TOTAL_SECTORS: u32 = 1024;

/// Maximum inodes
const MAX_INODES: u32 = 256;

/// Maximum data blocks
const MAX_DATA_BLOCKS: u32 = TOTAL_SECTORS - 35; // 989 blocks

/// Superblock structure (sector 0)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct Superblock {
    magic: u32,
    version: u32,
    block_size: u32,
    total_blocks: u32,
    inode_count: u32,
    free_blocks: u32,
    free_inodes: u32,
    first_data_block: u32,
    inode_bitmap_sector: u32,
    block_bitmap_sector: u32,
    inode_table_sector: u32,
    root_inode: u32,
    padding: [u8; 468],
}

/// Inode structure (64 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct Inode {
    pub inode_num: u32,
    pub file_type: u32,
    pub mode: u16,
    pub uid: u16,
    pub gid: u16,
    pub link_count: u16,
    pub size: u32,
    pub blocks_used: u32,
    pub direct_blocks: [u32; MAX_DIRECT_BLOCKS],
    padding: [u8; 8],
}

/// Directory entry structure (256 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct DirEntry {
    inode: u32,
    name_len: u8,
    file_type: u8,
    padding: u16,
    name: [u8; MAX_FILENAME_LEN],
}

/// File type constants
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FileType {
    File = 1,
    Directory = 2,
}

impl FileType {
    fn from_u32(val: u32) -> Option<Self> {
        match val {
            1 => Some(FileType::File),
            2 => Some(FileType::Directory),
            _ => None,
        }
    }
}

/// Home filesystem error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeFsError {
    IoError,
    NotFound,
    AlreadyExists,
    NotDirectory,
    IsDirectory,
    NoSpace,
    InvalidArgument,
    NotEmpty,
    Corrupted,
}

impl From<BlockError> for HomeFsError {
    fn from(_: BlockError) -> Self {
        HomeFsError::IoError
    }
}

/// Home filesystem
pub struct HomeFs<D: BlockDevice> {
    device: D,
    superblock: Superblock,
    inode_bitmap: [u8; 512],
    block_bitmap: [u8; 512],
}

impl<D: BlockDevice> HomeFs<D> {
    /// Create a new filesystem on the device
    pub fn create(mut device: D) -> Result<Self, HomeFsError> {
        // Create superblock
        let superblock = Superblock {
            magic: FS_MAGIC,
            version: FS_VERSION,
            block_size: BLOCK_SIZE as u32,
            total_blocks: TOTAL_SECTORS,
            inode_count: MAX_INODES,
            free_blocks: MAX_DATA_BLOCKS - 1, // -1 for root directory data
            free_inodes: MAX_INODES - 1,      // -1 for root directory
            first_data_block: 35,
            inode_bitmap_sector: 1,
            block_bitmap_sector: 2,
            inode_table_sector: 3,
            root_inode: 1,
            padding: [0; 468],
        };

        // Initialize bitmaps
        let mut inode_bitmap = [0u8; 512];
        let mut block_bitmap = [0u8; 512];

        // Mark root inode as used (inode 1)
        inode_bitmap[0] |= 0x02; // Bit 1

        // Mark first data block as used (for root directory)
        block_bitmap[0] |= 0x01; // Bit 0

        // Write superblock
        Self::write_superblock(&mut device, &superblock)?;

        // Write bitmaps
        device.write_sector(1, &inode_bitmap)?;
        device.write_sector(2, &block_bitmap)?;

        // Create root inode
        let root_inode = Inode {
            inode_num: 1,
            file_type: FileType::Directory as u32,
            mode: 0o755,
            uid: 0,
            gid: 0,
            link_count: 1,
            size: BLOCK_SIZE as u32,
            blocks_used: 1,
            direct_blocks: [35, 0, 0, 0, 0, 0, 0, 0], // First data block
            padding: [0; 8],
        };

        // Write root inode
        Self::write_inode_direct(&mut device, &root_inode)?;

        // Initialize root directory with . and ..
        let mut root_block = [0u8; 512];
        let mut offset = 0;

        // . entry
        let dot_entry = DirEntry {
            inode: 1,
            name_len: 1,
            file_type: FileType::Directory as u8,
            padding: 0,
            name: {
                let mut name = [0u8; MAX_FILENAME_LEN];
                name[0] = b'.';
                name
            },
        };
        Self::write_dir_entry_to_buf(&mut root_block[offset..], &dot_entry);
        offset += DIR_ENTRY_SIZE;

        // .. entry (parent is also root)
        let dotdot_entry = DirEntry {
            inode: 1,
            name_len: 2,
            file_type: FileType::Directory as u8,
            padding: 0,
            name: {
                let mut name = [0u8; MAX_FILENAME_LEN];
                name[0] = b'.';
                name[1] = b'.';
                name
            },
        };
        Self::write_dir_entry_to_buf(&mut root_block[offset..], &dotdot_entry);

        // Write root directory data
        device.write_sector(35, &root_block)?;

        Ok(Self { device, superblock, inode_bitmap, block_bitmap })
    }

    /// Open an existing filesystem
    pub fn open(mut device: D) -> Result<Self, HomeFsError> {
        // Read superblock
        let superblock = Self::read_superblock(&mut device)?;

        // Validate superblock
        if superblock.magic != FS_MAGIC {
            return Err(HomeFsError::Corrupted);
        }
        if superblock.version != FS_VERSION {
            return Err(HomeFsError::InvalidArgument);
        }

        // Read bitmaps
        let mut inode_bitmap = [0u8; 512];
        let mut block_bitmap = [0u8; 512];
        device.read_sector(1, &mut inode_bitmap)?;
        device.read_sector(2, &mut block_bitmap)?;

        Ok(Self { device, superblock, inode_bitmap, block_bitmap })
    }

    /// Read superblock from device
    fn read_superblock(device: &mut D) -> Result<Superblock, HomeFsError> {
        let mut buf = [0u8; SECTOR_SIZE];
        device.read_sector(0, &mut buf)?;

        // SAFETY: Superblock is repr(C, packed) and fits in 512 bytes
        let superblock = unsafe { core::ptr::read(buf.as_ptr() as *const Superblock) };
        Ok(superblock)
    }

    /// Write superblock to device
    fn write_superblock(device: &mut D, sb: &Superblock) -> Result<(), HomeFsError> {
        let mut buf = [0u8; SECTOR_SIZE];
        // SAFETY: Superblock is repr(C, packed) and fits in 512 bytes
        unsafe {
            core::ptr::copy_nonoverlapping(
                sb as *const Superblock as *const u8,
                buf.as_mut_ptr(),
                core::mem::size_of::<Superblock>(),
            );
        }
        device.write_sector(0, &buf)?;
        Ok(())
    }

    /// Read an inode from device
    pub fn read_inode(&mut self, inode_num: u32) -> Result<Inode, HomeFsError> {
        if inode_num == 0 || inode_num > MAX_INODES {
            return Err(HomeFsError::InvalidArgument);
        }

        let sector = 3 + (inode_num - 1) / INODES_PER_SECTOR as u32;
        let offset = ((inode_num - 1) % INODES_PER_SECTOR as u32) as usize * INODE_SIZE;

        let mut buf = [0u8; SECTOR_SIZE];
        self.device.read_sector(sector.into(), &mut buf)?;

        // SAFETY: Inode is repr(C, packed)
        let inode = unsafe { core::ptr::read((buf.as_ptr().add(offset)) as *const Inode) };

        Ok(inode)
    }

    /// Write an inode to device
    fn write_inode(&mut self, inode: &Inode) -> Result<(), HomeFsError> {
        Self::write_inode_direct(&mut self.device, inode)
    }

    /// Write an inode directly to device (static helper)
    fn write_inode_direct(device: &mut D, inode: &Inode) -> Result<(), HomeFsError> {
        let inode_num = inode.inode_num;
        if inode_num == 0 || inode_num > MAX_INODES {
            return Err(HomeFsError::InvalidArgument);
        }

        let sector = 3 + (inode_num - 1) / INODES_PER_SECTOR as u32;
        let offset = ((inode_num - 1) % INODES_PER_SECTOR as u32) as usize * INODE_SIZE;

        // Read sector, modify inode, write back
        let mut buf = [0u8; SECTOR_SIZE];
        device.read_sector(sector.into(), &mut buf)?;

        // SAFETY: Inode is repr(C, packed)
        unsafe {
            core::ptr::copy_nonoverlapping(
                inode as *const Inode as *const u8,
                buf.as_mut_ptr().add(offset),
                INODE_SIZE,
            );
        }

        device.write_sector(sector.into(), &buf)?;
        Ok(())
    }

    /// Helper to write directory entry to buffer (static method)
    fn write_dir_entry_to_buf(buf: &mut [u8], entry: &DirEntry) {
        assert!(buf.len() >= DIR_ENTRY_SIZE, "Buffer too small for directory entry");
        // SAFETY: DirEntry is repr(C, packed) and we verified buffer size
        unsafe {
            core::ptr::copy_nonoverlapping(
                entry as *const DirEntry as *const u8,
                buf.as_mut_ptr(),
                DIR_ENTRY_SIZE,
            );
        }
    }

    /// Allocate a free inode
    fn allocate_inode(&mut self) -> Result<u32, HomeFsError> {
        if self.superblock.free_inodes == 0 {
            return Err(HomeFsError::NoSpace);
        }

        // Find first free bit in inode bitmap
        for byte_idx in 0..32 {
            // Only check first 256 bits
            let byte = self.inode_bitmap[byte_idx];
            if byte != 0xFF {
                // Has free bits
                for bit in 0..8 {
                    if (byte & (1 << bit)) == 0 {
                        // Found free inode
                        let inode_num = (byte_idx * 8 + bit) as u32;
                        if inode_num == 0 || inode_num >= MAX_INODES {
                            continue;
                        }

                        // Mark as used
                        self.inode_bitmap[byte_idx] |= 1 << bit;
                        self.superblock.free_inodes -= 1;

                        // Write bitmap and superblock
                        self.device.write_sector(1, &self.inode_bitmap)?;
                        Self::write_superblock(&mut self.device, &self.superblock)?;

                        return Ok(inode_num);
                    }
                }
            }
        }

        Err(HomeFsError::NoSpace)
    }

    /// Free an inode
    fn free_inode(&mut self, inode_num: u32) -> Result<(), HomeFsError> {
        if inode_num == 0 || inode_num >= MAX_INODES {
            return Err(HomeFsError::InvalidArgument);
        }

        let byte_idx = (inode_num / 8) as usize;
        let bit = (inode_num % 8) as u8;

        // Check if actually allocated
        if (self.inode_bitmap[byte_idx] & (1 << bit)) == 0 {
            return Err(HomeFsError::InvalidArgument);
        }

        // Mark as free
        self.inode_bitmap[byte_idx] &= !(1 << bit);
        self.superblock.free_inodes += 1;

        // Write bitmap and superblock
        self.device.write_sector(1, &self.inode_bitmap)?;
        Self::write_superblock(&mut self.device, &self.superblock)?;

        Ok(())
    }

    /// Allocate a free data block
    fn allocate_block(&mut self) -> Result<u32, HomeFsError> {
        if self.superblock.free_blocks == 0 {
            return Err(HomeFsError::NoSpace);
        }

        // Find first free bit in block bitmap
        let max_bytes = (MAX_DATA_BLOCKS / 8) as usize;
        for byte_idx in 0..max_bytes {
            let byte = self.block_bitmap[byte_idx];
            if byte != 0xFF {
                // Has free bits
                for bit in 0..8 {
                    if (byte & (1 << bit)) == 0 {
                        // Found free block
                        let block_num = (byte_idx * 8 + bit) as u32;
                        if block_num >= MAX_DATA_BLOCKS {
                            continue;
                        }

                        // Mark as used
                        self.block_bitmap[byte_idx] |= 1 << bit;
                        self.superblock.free_blocks -= 1;

                        // Write bitmap and superblock
                        self.device.write_sector(2, &self.block_bitmap)?;
                        Self::write_superblock(&mut self.device, &self.superblock)?;

                        return Ok(block_num);
                    }
                }
            }
        }

        Err(HomeFsError::NoSpace)
    }

    /// Free a data block
    fn free_block(&mut self, block_num: u32) -> Result<(), HomeFsError> {
        if block_num >= MAX_DATA_BLOCKS {
            return Err(HomeFsError::InvalidArgument);
        }

        let byte_idx = (block_num / 8) as usize;
        let bit = (block_num % 8) as u8;

        // Check if actually allocated
        if (self.block_bitmap[byte_idx] & (1 << bit)) == 0 {
            return Err(HomeFsError::InvalidArgument);
        }

        // Mark as free
        self.block_bitmap[byte_idx] &= !(1 << bit);
        self.superblock.free_blocks += 1;

        // Write bitmap and superblock
        self.device.write_sector(2, &self.block_bitmap)?;
        Self::write_superblock(&mut self.device, &self.superblock)?;

        Ok(())
    }

    /// Get root inode number
    pub fn root_inode(&self) -> u32 {
        self.superblock.root_inode
    }

    /// Get filesystem statistics
    pub fn statfs(&self) -> (u32, u32, u32, u32) {
        (self.superblock.free_blocks, MAX_DATA_BLOCKS, self.superblock.free_inodes, MAX_INODES)
    }

    /// Read a directory entry at given index
    fn read_dir_entry(&mut self, block_sector: u64, index: usize) -> Result<DirEntry, HomeFsError> {
        let mut buf = [0u8; SECTOR_SIZE];
        self.device.read_sector(block_sector, &mut buf)?;

        let offset = index * DIR_ENTRY_SIZE;
        if offset + DIR_ENTRY_SIZE > SECTOR_SIZE {
            return Err(HomeFsError::InvalidArgument);
        }

        // SAFETY: DirEntry is repr(C, packed)
        let entry = unsafe { core::ptr::read((buf.as_ptr().add(offset)) as *const DirEntry) };
        Ok(entry)
    }

    /// Write a directory entry at given index
    fn write_dir_entry(
        &mut self,
        block_sector: u64,
        index: usize,
        entry: &DirEntry,
    ) -> Result<(), HomeFsError> {
        // Read entire block
        let mut buf = [0u8; SECTOR_SIZE];
        self.device.read_sector(block_sector, &mut buf)?;

        let offset = index * DIR_ENTRY_SIZE;
        if offset + DIR_ENTRY_SIZE > SECTOR_SIZE {
            return Err(HomeFsError::InvalidArgument);
        }

        // Write entry
        unsafe {
            core::ptr::copy_nonoverlapping(
                entry as *const DirEntry as *const u8,
                buf.as_mut_ptr().add(offset),
                DIR_ENTRY_SIZE,
            );
        }

        // Write back
        self.device.write_sector(block_sector, &buf)?;
        Ok(())
    }

    /// Lookup a file/directory in a directory by name
    pub fn lookup_in_dir(&mut self, dir_inode_num: u32, name: &str) -> Result<u32, HomeFsError> {
        if name.len() > MAX_FILENAME_LEN {
            return Err(HomeFsError::InvalidArgument);
        }

        let dir_inode = self.read_inode(dir_inode_num)?;
        if dir_inode.file_type != FileType::Directory as u32 {
            return Err(HomeFsError::NotDirectory);
        }

        // Search through directory entries
        for block_idx in 0..dir_inode.blocks_used as usize {
            if block_idx >= MAX_DIRECT_BLOCKS {
                break;
            }

            let block_num = dir_inode.direct_blocks[block_idx];
            if block_num == 0 {
                break;
            }

            let block_sector = self.superblock.first_data_block + block_num;

            for entry_idx in 0..ENTRIES_PER_BLOCK {
                let entry = self.read_dir_entry(block_sector.into(), entry_idx)?;

                if entry.inode == 0 {
                    continue; // Empty slot
                }

                // Compare name
                let entry_name = core::str::from_utf8(&entry.name[..entry.name_len as usize])
                    .map_err(|_| HomeFsError::Corrupted)?;

                if entry_name == name {
                    return Ok(entry.inode);
                }
            }
        }

        Err(HomeFsError::NotFound)
    }

    /// Add an entry to a directory
    pub fn add_dir_entry(
        &mut self,
        dir_inode_num: u32,
        name: &str,
        child_inode: u32,
        child_type: FileType,
    ) -> Result<(), HomeFsError> {
        if name.is_empty() || name.len() > MAX_FILENAME_LEN {
            return Err(HomeFsError::InvalidArgument);
        }

        // Check if already exists
        if self.lookup_in_dir(dir_inode_num, name).is_ok() {
            return Err(HomeFsError::AlreadyExists);
        }

        let mut dir_inode = self.read_inode(dir_inode_num)?;
        if dir_inode.file_type != FileType::Directory as u32 {
            return Err(HomeFsError::NotDirectory);
        }

        // Find empty slot or allocate new block
        for block_idx in 0..dir_inode.blocks_used as usize {
            if block_idx >= MAX_DIRECT_BLOCKS {
                return Err(HomeFsError::NoSpace);
            }

            let block_num = dir_inode.direct_blocks[block_idx];
            let block_sector = self.superblock.first_data_block + block_num;

            for entry_idx in 0..ENTRIES_PER_BLOCK {
                let entry = self.read_dir_entry(block_sector.into(), entry_idx)?;

                if entry.inode == 0 {
                    // Found empty slot
                    let new_entry = DirEntry {
                        inode: child_inode,
                        name_len: name.len() as u8,
                        file_type: child_type as u8,
                        padding: 0,
                        name: {
                            let mut name_buf = [0u8; MAX_FILENAME_LEN];
                            name_buf[..name.len()].copy_from_slice(name.as_bytes());
                            name_buf
                        },
                    };

                    self.write_dir_entry(block_sector.into(), entry_idx, &new_entry)?;
                    return Ok(());
                }
            }
        }

        // Need to allocate a new block
        if dir_inode.blocks_used as usize >= MAX_DIRECT_BLOCKS {
            return Err(HomeFsError::NoSpace);
        }

        let new_block = self.allocate_block()?;
        dir_inode.direct_blocks[dir_inode.blocks_used as usize] = new_block;
        dir_inode.blocks_used += 1;
        dir_inode.size = dir_inode.blocks_used * BLOCK_SIZE as u32;

        // Zero out new block
        let zero_block = [0u8; 512];
        let block_sector = self.superblock.first_data_block + new_block;
        self.device.write_sector(block_sector.into(), &zero_block)?;

        // Add entry to first slot of new block
        let new_entry = DirEntry {
            inode: child_inode,
            name_len: name.len() as u8,
            file_type: child_type as u8,
            padding: 0,
            name: {
                let mut name_buf = [0u8; MAX_FILENAME_LEN];
                name_buf[..name.len()].copy_from_slice(name.as_bytes());
                name_buf
            },
        };

        self.write_dir_entry(block_sector.into(), 0, &new_entry)?;

        // Write updated directory inode
        self.write_inode(&dir_inode)?;

        Ok(())
    }

    /// Create a new file
    pub fn create_file(
        &mut self,
        dir_inode_num: u32,
        name: &str,
        mode: u16,
    ) -> Result<u32, HomeFsError> {
        // Allocate inode
        let inode_num = self.allocate_inode()?;

        // Create inode
        let inode = Inode {
            inode_num,
            file_type: FileType::File as u32,
            mode,
            uid: 0,
            gid: 0,
            link_count: 1,
            size: 0,
            blocks_used: 0,
            direct_blocks: [0; MAX_DIRECT_BLOCKS],
            padding: [0; 8],
        };

        // Write inode
        self.write_inode(&inode)?;

        // Add directory entry
        if let Err(e) = self.add_dir_entry(dir_inode_num, name, inode_num, FileType::File) {
            // Rollback: free inode
            let _ = self.free_inode(inode_num);
            return Err(e);
        }

        Ok(inode_num)
    }

    /// Create a new directory
    pub fn create_directory(
        &mut self,
        parent_inode_num: u32,
        name: &str,
        mode: u16,
    ) -> Result<u32, HomeFsError> {
        // Allocate inode
        let inode_num = self.allocate_inode()?;

        // Allocate data block for directory
        let block_num = match self.allocate_block() {
            Ok(b) => b,
            Err(e) => {
                let _ = self.free_inode(inode_num);
                return Err(e);
            }
        };

        // Create inode
        let inode = Inode {
            inode_num,
            file_type: FileType::Directory as u32,
            mode,
            uid: 0,
            gid: 0,
            link_count: 1,
            size: BLOCK_SIZE as u32,
            blocks_used: 1,
            direct_blocks: {
                let mut blocks = [0; MAX_DIRECT_BLOCKS];
                blocks[0] = block_num;
                blocks
            },
            padding: [0; 8],
        };

        // Initialize directory with . and ..
        let block_sector = self.superblock.first_data_block + block_num;
        let mut dir_block = [0u8; 512];

        // . entry
        let dot_entry = DirEntry {
            inode: inode_num,
            name_len: 1,
            file_type: FileType::Directory as u8,
            padding: 0,
            name: {
                let mut name = [0u8; MAX_FILENAME_LEN];
                name[0] = b'.';
                name
            },
        };
        Self::write_dir_entry_to_buf(&mut dir_block[0..], &dot_entry);

        // .. entry
        let dotdot_entry = DirEntry {
            inode: parent_inode_num,
            name_len: 2,
            file_type: FileType::Directory as u8,
            padding: 0,
            name: {
                let mut name = [0u8; MAX_FILENAME_LEN];
                name[0] = b'.';
                name[1] = b'.';
                name
            },
        };
        Self::write_dir_entry_to_buf(&mut dir_block[DIR_ENTRY_SIZE..], &dotdot_entry);

        // Write directory data
        self.device.write_sector(block_sector.into(), &dir_block)?;

        // Write inode
        self.write_inode(&inode)?;

        // Add directory entry to parent
        if let Err(e) = self.add_dir_entry(parent_inode_num, name, inode_num, FileType::Directory) {
            // Rollback
            let _ = self.free_block(block_num);
            let _ = self.free_inode(inode_num);
            return Err(e);
        }

        Ok(inode_num)
    }

    /// Read file data
    pub fn read_file(
        &mut self,
        inode_num: u32,
        offset: usize,
        buf: &mut [u8],
    ) -> Result<usize, HomeFsError> {
        let inode = self.read_inode(inode_num)?;

        if inode.file_type != FileType::File as u32 {
            return Err(HomeFsError::IsDirectory);
        }

        let file_size = inode.size as usize;
        if offset >= file_size {
            return Ok(0); // EOF
        }

        let to_read = core::cmp::min(buf.len(), file_size - offset);
        let mut bytes_read = 0;

        while bytes_read < to_read {
            let block_idx = (offset + bytes_read) / BLOCK_SIZE;
            let block_offset = (offset + bytes_read) % BLOCK_SIZE;

            if block_idx >= MAX_DIRECT_BLOCKS || block_idx >= inode.blocks_used as usize {
                break;
            }

            let block_num = inode.direct_blocks[block_idx];
            if block_num == 0 {
                break;
            }

            let block_sector = self.superblock.first_data_block + block_num;
            let mut block_buf = [0u8; SECTOR_SIZE];
            self.device.read_sector(block_sector.into(), &mut block_buf)?;

            let chunk_size = core::cmp::min(BLOCK_SIZE - block_offset, to_read - bytes_read);
            buf[bytes_read..bytes_read + chunk_size]
                .copy_from_slice(&block_buf[block_offset..block_offset + chunk_size]);

            bytes_read += chunk_size;
        }

        Ok(bytes_read)
    }

    /// Write file data
    pub fn write_file(
        &mut self,
        inode_num: u32,
        offset: usize,
        buf: &[u8],
    ) -> Result<usize, HomeFsError> {
        let mut inode = self.read_inode(inode_num)?;

        if inode.file_type != FileType::File as u32 {
            return Err(HomeFsError::IsDirectory);
        }

        let end_offset = offset + buf.len();
        let blocks_needed = (end_offset + BLOCK_SIZE - 1) / BLOCK_SIZE;

        if blocks_needed > MAX_DIRECT_BLOCKS {
            return Err(HomeFsError::NoSpace);
        }

        // Allocate blocks if needed
        while (inode.blocks_used as usize) < blocks_needed {
            let new_block = self.allocate_block()?;
            inode.direct_blocks[inode.blocks_used as usize] = new_block;
            inode.blocks_used += 1;
        }

        // Write data
        let mut bytes_written = 0;
        while bytes_written < buf.len() {
            let block_idx = (offset + bytes_written) / BLOCK_SIZE;
            let block_offset = (offset + bytes_written) % BLOCK_SIZE;

            if block_idx >= MAX_DIRECT_BLOCKS {
                break;
            }

            let block_num = inode.direct_blocks[block_idx];
            if block_num == 0 {
                break;
            }

            let block_sector = self.superblock.first_data_block + block_num;

            // Read existing block
            let mut block_buf = [0u8; SECTOR_SIZE];
            self.device.read_sector(block_sector.into(), &mut block_buf)?;

            // Modify block
            let chunk_size = core::cmp::min(BLOCK_SIZE - block_offset, buf.len() - bytes_written);
            block_buf[block_offset..block_offset + chunk_size]
                .copy_from_slice(&buf[bytes_written..bytes_written + chunk_size]);

            // Write back
            self.device.write_sector(block_sector.into(), &block_buf)?;

            bytes_written += chunk_size;
        }

        // Update inode size if expanded
        if end_offset > inode.size as usize {
            inode.size = end_offset as u32;
        }

        self.write_inode(&inode)?;

        Ok(bytes_written)
    }

    /// Remove a directory entry
    fn remove_dir_entry(&mut self, dir_inode_num: u32, name: &str) -> Result<(), HomeFsError> {
        let dir_inode = self.read_inode(dir_inode_num)?;
        if dir_inode.file_type != FileType::Directory as u32 {
            return Err(HomeFsError::NotDirectory);
        }

        // Find and remove entry
        for block_idx in 0..dir_inode.blocks_used as usize {
            if block_idx >= MAX_DIRECT_BLOCKS {
                break;
            }

            let block_num = dir_inode.direct_blocks[block_idx];
            if block_num == 0 {
                break;
            }

            let block_sector = self.superblock.first_data_block + block_num;

            for entry_idx in 0..ENTRIES_PER_BLOCK {
                let entry = self.read_dir_entry(block_sector.into(), entry_idx)?;

                if entry.inode == 0 {
                    continue;
                }

                // Compare name
                let entry_name = core::str::from_utf8(&entry.name[..entry.name_len as usize])
                    .map_err(|_| HomeFsError::Corrupted)?;

                if entry_name == name {
                    // Found it - mark as empty
                    let empty_entry = DirEntry {
                        inode: 0,
                        name_len: 0,
                        file_type: 0,
                        padding: 0,
                        name: [0; MAX_FILENAME_LEN],
                    };
                    self.write_dir_entry(block_sector.into(), entry_idx, &empty_entry)?;
                    return Ok(());
                }
            }
        }

        Err(HomeFsError::NotFound)
    }

    /// Delete a file
    pub fn unlink_file(&mut self, dir_inode_num: u32, name: &str) -> Result<(), HomeFsError> {
        // Lookup file
        let file_inode_num = self.lookup_in_dir(dir_inode_num, name)?;
        let file_inode = self.read_inode(file_inode_num)?;

        if file_inode.file_type == FileType::Directory as u32 {
            return Err(HomeFsError::IsDirectory);
        }

        // Free all blocks
        for block_idx in 0..file_inode.blocks_used as usize {
            if block_idx >= MAX_DIRECT_BLOCKS {
                break;
            }
            let block_num = file_inode.direct_blocks[block_idx];
            if block_num != 0 {
                self.free_block(block_num)?;
            }
        }

        // Free inode
        self.free_inode(file_inode_num)?;

        // Remove directory entry
        self.remove_dir_entry(dir_inode_num, name)?;

        Ok(())
    }

    /// Delete an empty directory
    pub fn rmdir(&mut self, parent_inode_num: u32, name: &str) -> Result<(), HomeFsError> {
        // Lookup directory
        let dir_inode_num = self.lookup_in_dir(parent_inode_num, name)?;
        let dir_inode = self.read_inode(dir_inode_num)?;

        if dir_inode.file_type != FileType::Directory as u32 {
            return Err(HomeFsError::NotDirectory);
        }

        // Check if empty (only . and .. entries)
        let mut entry_count = 0;
        for block_idx in 0..dir_inode.blocks_used as usize {
            if block_idx >= MAX_DIRECT_BLOCKS {
                break;
            }

            let block_num = dir_inode.direct_blocks[block_idx];
            if block_num == 0 {
                break;
            }

            let block_sector = self.superblock.first_data_block + block_num;

            for entry_idx in 0..ENTRIES_PER_BLOCK {
                let entry = self.read_dir_entry(block_sector.into(), entry_idx)?;

                if entry.inode == 0 {
                    continue;
                }

                entry_count += 1;
            }
        }

        // Should only have . and .. (2 entries)
        if entry_count > 2 {
            return Err(HomeFsError::NotEmpty);
        }

        // Free all blocks
        for block_idx in 0..dir_inode.blocks_used as usize {
            if block_idx >= MAX_DIRECT_BLOCKS {
                break;
            }
            let block_num = dir_inode.direct_blocks[block_idx];
            if block_num > 0 {
                self.free_block(block_num)?;
            }
        }

        // Free inode
        self.free_inode(dir_inode_num)?;

        // Remove directory entry from parent
        self.remove_dir_entry(parent_inode_num, name)?;

        Ok(())
    }

    /// Rename a file or directory within the filesystem
    pub fn rename(
        &mut self,
        old_dir: u32,
        old_name: &str,
        new_dir: u32,
        new_name: &str,
    ) -> Result<(), HomeFsError> {
        // Lookup old entry
        let inode_num = self.lookup_in_dir(old_dir, old_name)?;
        let inode = self.read_inode(inode_num)?;

        // Check if new name already exists
        if self.lookup_in_dir(new_dir, new_name).is_ok() {
            return Err(HomeFsError::AlreadyExists);
        }

        // Add new entry
        let file_type = if inode.file_type == FileType::Directory as u32 {
            FileType::Directory
        } else {
            FileType::File
        };
        self.add_dir_entry(new_dir, new_name, inode_num, file_type)?;

        // Remove old entry
        self.remove_dir_entry(old_dir, old_name)?;

        Ok(())
    }

    /// List directory entries
    pub fn readdir(
        &mut self,
        dir_inode_num: u32,
    ) -> Result<alloc::vec::Vec<(u32, alloc::string::String, FileType)>, HomeFsError> {
        let dir_inode = self.read_inode(dir_inode_num)?;
        if dir_inode.file_type != FileType::Directory as u32 {
            return Err(HomeFsError::NotDirectory);
        }

        let mut entries = alloc::vec::Vec::new();

        for block_idx in 0..dir_inode.blocks_used as usize {
            if block_idx >= MAX_DIRECT_BLOCKS {
                break;
            }

            let block_num = dir_inode.direct_blocks[block_idx];
            if block_num == 0 {
                break;
            }

            let block_sector = self.superblock.first_data_block + block_num;

            for entry_idx in 0..ENTRIES_PER_BLOCK {
                let entry = self.read_dir_entry(block_sector.into(), entry_idx)?;

                if entry.inode == 0 {
                    continue;
                }

                let name = core::str::from_utf8(&entry.name[..entry.name_len as usize])
                    .map_err(|_| HomeFsError::Corrupted)?;

                let file_type =
                    FileType::from_u32(entry.file_type as u32).ok_or(HomeFsError::Corrupted)?;

                entries.push((entry.inode, alloc::string::String::from(name), file_type));
            }
        }

        Ok(entries)
    }

    /// Truncate a file to zero length
    pub fn truncate(&mut self, inode_num: u32) -> Result<(), HomeFsError> {
        let mut inode = self.read_inode(inode_num)?;

        if inode.file_type != FileType::File as u32 {
            return Err(HomeFsError::IsDirectory);
        }

        // Free all blocks
        for block_idx in 0..inode.blocks_used as usize {
            if block_idx >= MAX_DIRECT_BLOCKS {
                break;
            }
            let block_num = inode.direct_blocks[block_idx];
            if block_num > 0 {
                self.free_block(block_num)?;
            }
        }

        // Update inode
        inode.size = 0;
        inode.blocks_used = 0;
        inode.direct_blocks = [0; MAX_DIRECT_BLOCKS];

        self.write_inode(&inode)?;

        Ok(())
    }

    /// Set file permissions
    pub fn chmod(&mut self, inode_num: u32, mode: u16) -> Result<(), HomeFsError> {
        let mut inode = self.read_inode(inode_num)?;
        inode.mode = mode & 0o777; // Only permission bits
        self.write_inode(&inode)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock block device for testing
    struct MockDevice {
        data: Vec<[u8; 512]>,
    }

    impl MockDevice {
        fn new(size: usize) -> Self {
            Self { data: vec![[0u8; 512]; size] }
        }
    }

    impl BlockDevice for MockDevice {
        fn read_sector(&mut self, sector: u64, buffer: &mut [u8; 512]) -> Result<(), BlockError> {
            if sector as usize >= self.data.len() {
                return Err(BlockError::InvalidSector);
            }
            buffer.copy_from_slice(&self.data[sector as usize]);
            Ok(())
        }

        fn write_sector(&mut self, sector: u64, buffer: &[u8; 512]) -> Result<(), BlockError> {
            if sector as usize >= self.data.len() {
                return Err(BlockError::InvalidSector);
            }
            self.data[sector as usize].copy_from_slice(buffer);
            Ok(())
        }
    }

    #[test]
    fn test_create_filesystem() {
        let device = MockDevice::new(1024);
        let fs = HomeFs::create(device).unwrap();
        assert_eq!(fs.superblock.magic, FS_MAGIC);
        assert_eq!(fs.superblock.version, FS_VERSION);
        assert_eq!(fs.superblock.root_inode, 1);
    }

    #[test]
    fn test_open_filesystem() {
        let device = MockDevice::new(1024);
        let _fs = HomeFs::create(device).unwrap();
        // Re-open would require extracting device which we can't do easily
        // This test is more of a compile check
    }

    #[test]
    fn test_allocate_inode() {
        let device = MockDevice::new(1024);
        let mut fs = HomeFs::create(device).unwrap();

        let inode1 = fs.allocate_inode().unwrap();
        assert_eq!(inode1, 2); // Root is 1, so next is 2

        let inode2 = fs.allocate_inode().unwrap();
        assert_eq!(inode2, 3);
    }

    #[test]
    fn test_allocate_block() {
        let device = MockDevice::new(1024);
        let mut fs = HomeFs::create(device).unwrap();

        let block1 = fs.allocate_block().unwrap();
        assert_eq!(block1, 1); // Block 0 used by root dir

        let block2 = fs.allocate_block().unwrap();
        assert_eq!(block2, 2);
    }
}
