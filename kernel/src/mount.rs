//! Mount point management for VFS
//!
//! This module provides a simple mount table that maps mount points
//! to filesystem instances (disk filesystem and tmpfs).

use crate::diskfs::{DiskFs, DiskFsError, InodeType};
use crate::fs::{FileMetadata, FileType};
use crate::syscall::ErrorCode;
use crate::tmpfs::Inode as TmpfsInode;
use alloc::string::String;
use alloc::vec::Vec;
use panda_hal::ata::AtaDisk;
use spin::Mutex;

/// Filesystem type
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FsType {
    Disk,
    Tmpfs,
}

/// Global mount table
static MOUNT_TABLE: Mutex<Option<MountTable>> = Mutex::new(None);

/// Mount table entry
#[derive(Clone, Copy, Debug)]
pub struct MountEntry {
    /// Mount point path (e.g., "/mnt", "/tmp")
    mount_point: &'static str,
    /// Filesystem type
    fs_type: FsType,
}

/// Mount table with filesystem instances
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
        self.mounts
            .push((String::from(mount_point), MountEntry { mount_point, fs_type: FsType::Disk }));
        self.disk_fs = Some(disk_fs);

        Ok(())
    }

    /// Mount tmpfs at a mount point
    pub fn mount_tmpfs(&mut self, mount_point: &'static str) -> Result<(), ErrorCode> {
        // Add mount entry
        self.mounts
            .push((String::from(mount_point), MountEntry { mount_point, fs_type: FsType::Tmpfs }));
        Ok(())
    }

    /// Check if a path is within a mounted filesystem
    /// Returns the mount point, the relative path, and the filesystem type
    pub fn resolve_mount<'a>(&'a self, path: &'a str) -> Option<(&'a str, &'a str, FsType)> {
        for (mount_point, entry) in &self.mounts {
            if path == mount_point.as_str() {
                // Exact match - return root of mounted fs
                return Some((mount_point.as_str(), "/", entry.fs_type));
            }
            let mount_with_slash = alloc::format!("{}/", mount_point);
            if path.starts_with(&mount_with_slash) {
                // Path is within this mount point
                let relative = &path[mount_point.len()..];
                return Some((mount_point.as_str(), relative, entry.fs_type));
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

/// Mount tmpfs at /tmp
pub fn mount_tmpfs_at_tmp() -> Result<(), ErrorCode> {
    // Initialize tmpfs first
    crate::tmpfs::init_tmpfs();

    // Add to mount table
    let mut table = MOUNT_TABLE.lock();
    let mount_table = table.as_mut().ok_or(ErrorCode::EIO)?;
    mount_table.mount_tmpfs("/tmp")
}

/// Resolve a path that might be on a mounted filesystem
///
/// Returns:
/// - None if the path is in the in-memory VFS
/// - Some((mount_point, relative_path, fs_type)) if on a mounted filesystem
pub fn resolve_mount_path(path: &str) -> Option<(String, String, FsType)> {
    let table = MOUNT_TABLE.lock();
    let mount_table = table.as_ref()?;
    mount_table
        .resolve_mount(path)
        .map(|(mount, rel, fs_type)| (String::from(mount), String::from(rel), fs_type))
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

    let file_type = match file.file_type {
        InodeType::File => FileType::File,
        InodeType::Directory => FileType::Directory,
    };
    let mode = match file_type {
        FileType::Directory => crate::fs::DEFAULT_DIR_MODE,
        FileType::File => crate::fs::DEFAULT_FILE_MODE,
    };

    // Default ownership: root:root
    Ok(FileMetadata { file_type, size: file.size, mode, uid: 0, gid: 0 })
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

/// Tmpfs operations

/// Lookup a file in tmpfs by path
pub fn tmpfs_lookup(path: &str) -> Result<TmpfsInode, ErrorCode> {
    crate::tmpfs::with_tmpfs(|fs| {
        let root = fs.root_inode();
        // Remove leading slash from path
        let rel_path = path.strip_prefix('/').unwrap_or(path);
        fs.lookup(root, rel_path)
    })
}

/// Create a file or directory in tmpfs
pub fn tmpfs_create(parent_path: &str, name: &str, is_dir: bool) -> Result<TmpfsInode, ErrorCode> {
    crate::tmpfs::with_tmpfs(|fs| {
        let root = fs.root_inode();
        // Lookup parent directory
        let parent_inode = if parent_path == "/" || parent_path.is_empty() {
            root
        } else {
            let rel_path = parent_path.strip_prefix('/').unwrap_or(parent_path);
            fs.lookup(root, rel_path)?
        };

        // Create the file/dir
        fs.create(parent_inode, name, is_dir)
    })
}

/// Read from a tmpfs file
pub fn tmpfs_read(inode: TmpfsInode, offset: usize, buffer: &mut [u8]) -> Result<usize, ErrorCode> {
    crate::tmpfs::with_tmpfs(|fs| fs.read(inode, offset, buffer))
}

/// Write to a tmpfs file
pub fn tmpfs_write(inode: TmpfsInode, offset: usize, data: &[u8]) -> Result<usize, ErrorCode> {
    crate::tmpfs::with_tmpfs(|fs| fs.write(inode, offset, data))
}

/// Truncate a tmpfs file
pub fn tmpfs_truncate(inode: TmpfsInode) -> Result<(), ErrorCode> {
    crate::tmpfs::with_tmpfs(|fs| fs.truncate(inode))
}

/// Get file metadata from tmpfs
pub fn tmpfs_stat(inode: TmpfsInode) -> Result<FileMetadata, ErrorCode> {
    crate::tmpfs::with_tmpfs(|fs| fs.stat(inode))
}

/// List directory in tmpfs
pub fn tmpfs_list_dir(inode: TmpfsInode) -> Result<Vec<(String, FileType)>, ErrorCode> {
    crate::tmpfs::with_tmpfs(|fs| fs.read_dir(inode))
}

/// Unlink (delete) a file or empty directory from tmpfs
pub fn tmpfs_unlink(parent_path: &str, name: &str) -> Result<(), ErrorCode> {
    crate::tmpfs::with_tmpfs(|fs| {
        let root = fs.root_inode();
        // Lookup parent directory
        let parent_inode = if parent_path == "/" || parent_path.is_empty() {
            root
        } else {
            let rel_path = parent_path.strip_prefix('/').unwrap_or(parent_path);
            fs.lookup(root, rel_path)?
        };

        // Unlink the file
        fs.unlink(parent_inode, name)
    })
}

/// Change file mode (chmod) in tmpfs
pub fn tmpfs_chmod(inode: TmpfsInode, new_mode: u16) -> Result<(), ErrorCode> {
    crate::tmpfs::with_tmpfs(|fs| fs.chmod(inode, new_mode))
}

/// Create a directory in tmpfs
///
/// # Errors
///
/// Returns an error if the parent directory doesn't exist or the directory already exists
pub fn tmpfs_mkdir(parent_path: &str, name: &str) -> Result<TmpfsInode, ErrorCode> {
    tmpfs_create(parent_path, name, true)
}

/// Remove an empty directory from tmpfs
///
/// # Errors
///
/// Returns an error if the directory doesn't exist, is not empty, or is not a directory
pub fn tmpfs_rmdir(parent_path: &str, name: &str) -> Result<(), ErrorCode> {
    crate::tmpfs::with_tmpfs(|fs| {
        let root = fs.root_inode();
        // Lookup parent directory
        let parent_inode = if parent_path == "/" || parent_path.is_empty() {
            root
        } else {
            let rel_path = parent_path.strip_prefix('/').unwrap_or(parent_path);
            fs.lookup(root, rel_path)?
        };

        // Look up the directory to remove
        let dir_inode = fs.lookup(parent_inode, name)?;

        // Check if it's a directory
        let metadata = fs.stat(dir_inode)?;
        if metadata.file_type != FileType::Directory {
            return Err(ErrorCode::ENOTDIR);
        }

        // Check if it's empty
        let entries = fs.read_dir(dir_inode)?;
        if !entries.is_empty() {
            return Err(ErrorCode::ENOTEMPTY);
        }

        // Remove the directory
        fs.unlink(parent_inode, name)
    })
}

/// Rename a file or directory in tmpfs
///
/// # Errors
///
/// Returns an error if the old or new parent directories don't exist,
/// or if the new name already exists
pub fn tmpfs_rename(
    old_parent_path: &str,
    old_name: &str,
    new_parent_path: &str,
    new_name: &str,
) -> Result<(), ErrorCode> {
    crate::tmpfs::with_tmpfs(|fs| {
        let root = fs.root_inode();

        // Lookup old parent directory
        let old_parent_inode = if old_parent_path == "/" || old_parent_path.is_empty() {
            root
        } else {
            let rel_path = old_parent_path.strip_prefix('/').unwrap_or(old_parent_path);
            fs.lookup(root, rel_path)?
        };

        // Lookup new parent directory
        let new_parent_inode = if new_parent_path == "/" || new_parent_path.is_empty() {
            root
        } else {
            let rel_path = new_parent_path.strip_prefix('/').unwrap_or(new_parent_path);
            fs.lookup(root, rel_path)?
        };

        // Perform the rename
        fs.rename(old_parent_inode, old_name, new_parent_inode, new_name)
    })
}
