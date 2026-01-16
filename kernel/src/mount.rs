//! Mount point management for VFS
//!
//! This module provides a simple mount table that maps mount points
//! to disk filesystem instances.

use crate::diskfs::{DiskFs, DiskFsError, InodeType};
use crate::fs::{FileMetadata, FileType};
use crate::syscall::ErrorCode;
use alloc::string::String;
use alloc::vec::Vec;
use panda_hal::ata::AtaDisk;
use spin::Mutex;

/// Global mount table
static MOUNT_TABLE: Mutex<Option<MountTable>> = Mutex::new(None);

/// Mount table entry
#[derive(Clone, Copy, Debug)]
pub struct MountEntry {
    /// Mount point path (e.g., "/mnt")
    mount_point: &'static str,
}

/// Mount table with disk filesystem
pub struct MountTable {
    mounts: Vec<(String, MountEntry)>,
    disk_fs: Option<DiskFs<AtaDisk>>,
}

impl MountTable {
    /// Create a new empty mount table
    pub fn new() -> Self {
        Self { mounts: Vec::new(), disk_fs: None }
    }

    /// Mount a disk filesystem at a mount point
    pub fn mount_disk(&mut self, mount_point: &'static str) -> Result<(), ErrorCode> {
        // Initialize ATA disk
        // SAFETY: This should only be called once during boot
        let disk = unsafe { AtaDisk::new() };

        // Create disk filesystem
        let disk_fs = DiskFs::new(disk).map_err(|_| ErrorCode::EIO)?;

        // Add mount entry
        self.mounts.push((String::from(mount_point), MountEntry { mount_point }));
        self.disk_fs = Some(disk_fs);

        Ok(())
    }

    /// Check if a path is within a mounted filesystem
    /// Returns the mount point and the relative path within that filesystem
    pub fn resolve_mount<'a>(&'a self, path: &'a str) -> Option<(&'a str, &'a str)> {
        for (mount_point, _) in &self.mounts {
            if path == mount_point.as_str() {
                // Exact match - return root of mounted fs
                return Some((mount_point.as_str(), "/"));
            }
            let mount_with_slash = alloc::format!("{}/", mount_point);
            if path.starts_with(&mount_with_slash) {
                // Path is within this mount point
                let relative = &path[mount_point.len()..];
                return Some((mount_point.as_str(), relative));
            }
        }
        None
    }

    /// Get the disk filesystem (mutable)
    pub fn disk_fs_mut(&mut self) -> Option<&mut DiskFs<AtaDisk>> {
        self.disk_fs.as_mut()
    }

    /// Get the disk filesystem (immutable)
    pub fn disk_fs(&self) -> Option<&DiskFs<AtaDisk>> {
        self.disk_fs.as_ref()
    }
}

/// Initialize the mount table
pub fn init_mount_table() {
    let mut table = MOUNT_TABLE.lock();
    *table = Some(MountTable::new());
}

/// Mount the disk filesystem at /mnt
pub fn mount_disk_at_mnt() -> Result<(), ErrorCode> {
    let mut table = MOUNT_TABLE.lock();
    let mount_table = table.as_mut().ok_or(ErrorCode::EIO)?;
    mount_table.mount_disk("/mnt")
}

/// Resolve a path that might be on a mounted filesystem
///
/// Returns:
/// - None if the path is in the in-memory VFS
/// - Some((mount_point, relative_path)) if on a mounted filesystem
pub fn resolve_mount_path(path: &str) -> Option<(String, String)> {
    let table = MOUNT_TABLE.lock();
    let mount_table = table.as_ref()?;
    mount_table
        .resolve_mount(path)
        .map(|(mount, rel)| (String::from(mount), String::from(rel)))
}

/// Lookup a file on the disk filesystem
pub fn diskfs_lookup(path: &str) -> Result<u32, ErrorCode> {
    let mut table = MOUNT_TABLE.lock();
    let mount_table = table.as_mut().ok_or(ErrorCode::EIO)?;
    let disk_fs = mount_table.disk_fs_mut().ok_or(ErrorCode::EIO)?;

    disk_fs.resolve_path(path).map_err(diskfs_error_to_errno)
}

/// Read from a disk file
pub fn diskfs_read(inode: u32, offset: usize, buffer: &mut [u8]) -> Result<usize, ErrorCode> {
    let mut table = MOUNT_TABLE.lock();
    let mount_table = table.as_mut().ok_or(ErrorCode::EIO)?;
    let disk_fs = mount_table.disk_fs_mut().ok_or(ErrorCode::EIO)?;

    disk_fs.read_file(inode, offset, buffer).map_err(diskfs_error_to_errno)
}

/// Get file metadata from disk
pub fn diskfs_stat(inode: u32) -> Result<FileMetadata, ErrorCode> {
    let mut table = MOUNT_TABLE.lock();
    let mount_table = table.as_mut().ok_or(ErrorCode::EIO)?;
    let disk_fs = mount_table.disk_fs_mut().ok_or(ErrorCode::EIO)?;

    let file = disk_fs.stat(inode).map_err(diskfs_error_to_errno)?;

    Ok(FileMetadata {
        file_type: match file.file_type {
            InodeType::File => FileType::File,
            InodeType::Directory => FileType::Directory,
        },
        size: file.size,
    })
}

/// List directory on disk filesystem
pub fn diskfs_list_dir(inode: u32) -> Result<Vec<(String, FileType)>, ErrorCode> {
    let mut table = MOUNT_TABLE.lock();
    let mount_table = table.as_mut().ok_or(ErrorCode::EIO)?;
    let disk_fs = mount_table.disk_fs_mut().ok_or(ErrorCode::EIO)?;

    let entries = disk_fs.read_dir(inode).map_err(diskfs_error_to_errno)?;

    let mut result = Vec::new();
    for entry in entries {
        // Get file type for each entry
        let file = disk_fs.stat(entry.inode).map_err(diskfs_error_to_errno)?;
        let file_type = match file.file_type {
            InodeType::File => FileType::File,
            InodeType::Directory => FileType::Directory,
        };
        result.push((entry.name, file_type));
    }

    Ok(result)
}

/// Convert disk filesystem error to errno
fn diskfs_error_to_errno(err: DiskFsError) -> ErrorCode {
    match err {
        DiskFsError::IoError => ErrorCode::EIO,
        DiskFsError::InvalidMagic => ErrorCode::EINVAL,
        DiskFsError::InvalidVersion => ErrorCode::EINVAL,
        DiskFsError::InvalidInode => ErrorCode::ENOENT,
        DiskFsError::InvalidInodeType => ErrorCode::EINVAL,
        DiskFsError::InvalidPath => ErrorCode::EINVAL,
        DiskFsError::NotFound => ErrorCode::ENOENT,
        DiskFsError::NotADirectory => ErrorCode::ENOTDIR,
        DiskFsError::NotAFile => ErrorCode::EISDIR,
    }
}
