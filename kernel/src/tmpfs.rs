//! Temporary filesystem (tmpfs) - writable in-memory filesystem
//!
//! This module implements a simple in-memory filesystem with support for:
//! - File creation and deletion
//! - Reading and writing
//! - Directory operations
//!
//! All data is stored in memory and is lost on reboot.
//! The filesystem is mounted at /tmp by default.

use crate::fs::{FileMetadata, FileType};
use crate::syscall::ErrorCode;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Inode number type
pub type Inode = u32;

/// Tmpfs node types
#[derive(Clone, Debug)]
pub enum TmpFsNode {
    /// Regular file with data
    File { data: Vec<u8> },
    /// Directory with child entries (name -> inode)
    Directory { entries: BTreeMap<String, Inode> },
}

impl TmpFsNode {
    /// Create a new empty file
    pub fn new_file() -> Self {
        TmpFsNode::File { data: Vec::new() }
    }

    /// Create a new empty directory
    pub fn new_directory() -> Self {
        TmpFsNode::Directory { entries: BTreeMap::new() }
    }

    /// Get file type
    pub fn file_type(&self) -> FileType {
        match self {
            TmpFsNode::File { .. } => FileType::File,
            TmpFsNode::Directory { .. } => FileType::Directory,
        }
    }

    /// Get file size (0 for directories)
    pub fn size(&self) -> u64 {
        match self {
            TmpFsNode::File { data } => data.len() as u64,
            TmpFsNode::Directory { .. } => 0,
        }
    }
}

/// Tmpfs filesystem structure
pub struct TmpFs {
    /// Next available inode number
    next_inode: Inode,
    /// Map of inode to node
    nodes: BTreeMap<Inode, TmpFsNode>,
    /// Map of inode to mode bits
    modes: BTreeMap<Inode, u16>,
    /// Root directory inode
    root_inode: Inode,
}

impl TmpFs {
    /// Create a new tmpfs with an empty root directory
    pub fn new() -> Self {
        let root_inode = 1;
        let mut nodes = BTreeMap::new();
        nodes.insert(root_inode, TmpFsNode::new_directory());

        let mut modes = BTreeMap::new();
        modes.insert(root_inode, crate::fs::DEFAULT_DIR_MODE);

        Self { next_inode: root_inode + 1, nodes, modes, root_inode }
    }

    /// Get the root inode
    pub fn root_inode(&self) -> Inode {
        self.root_inode
    }

    /// Allocate a new inode number
    fn allocate_inode(&mut self) -> Inode {
        let inode = self.next_inode;
        self.next_inode += 1;
        inode
    }

    /// Lookup a path relative to an inode
    /// Path must be relative (no leading /)
    pub fn lookup(&self, base_inode: Inode, path: &str) -> Result<Inode, ErrorCode> {
        if path.is_empty() || path == "/" {
            return Ok(base_inode);
        }

        // Split path into components
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current_inode = base_inode;

        for component in components {
            // Get current node
            let node = self.nodes.get(&current_inode).ok_or(ErrorCode::ENOENT)?;

            // Must be a directory
            match node {
                TmpFsNode::Directory { entries } => {
                    // Look up component in directory
                    current_inode = *entries.get(component).ok_or(ErrorCode::ENOENT)?;
                }
                TmpFsNode::File { .. } => {
                    return Err(ErrorCode::ENOTDIR);
                }
            }
        }

        Ok(current_inode)
    }

    /// Create a new file or directory
    /// Parent directory must exist
    /// Returns the new inode number
    pub fn create(
        &mut self,
        parent_inode: Inode,
        name: &str,
        is_dir: bool,
    ) -> Result<Inode, ErrorCode> {
        // Validate name
        if name.is_empty() || name.contains('/') {
            return Err(ErrorCode::EINVAL);
        }

        // Check parent exists and is a directory
        {
            let parent = self.nodes.get(&parent_inode).ok_or(ErrorCode::ENOENT)?;
            match parent {
                TmpFsNode::Directory { entries } => {
                    // Check if entry already exists
                    if entries.contains_key(name) {
                        return Err(ErrorCode::EEXIST);
                    }
                }
                TmpFsNode::File { .. } => return Err(ErrorCode::ENOTDIR),
            }
        }

        // Allocate new inode
        let new_inode = self.allocate_inode();

        // Create new node
        let new_node = if is_dir { TmpFsNode::new_directory() } else { TmpFsNode::new_file() };

        // Add to nodes
        self.nodes.insert(new_inode, new_node);

        // Set default mode
        let default_mode =
            if is_dir { crate::fs::DEFAULT_DIR_MODE } else { crate::fs::DEFAULT_FILE_MODE };
        self.modes.insert(new_inode, default_mode);

        // Add to parent directory (now we can safely get mut again)
        let parent = self.nodes.get_mut(&parent_inode).ok_or(ErrorCode::ENOENT)?;
        match parent {
            TmpFsNode::Directory { entries } => {
                entries.insert(String::from(name), new_inode);
            }
            TmpFsNode::File { .. } => return Err(ErrorCode::ENOTDIR),
        }

        Ok(new_inode)
    }

    /// Delete a file or empty directory
    pub fn unlink(&mut self, parent_inode: Inode, name: &str) -> Result<(), ErrorCode> {
        // Validate name
        if name.is_empty() || name.contains('/') {
            return Err(ErrorCode::EINVAL);
        }

        // Look up the entry and validate
        let inode = {
            let parent = self.nodes.get(&parent_inode).ok_or(ErrorCode::ENOENT)?;
            let entries = match parent {
                TmpFsNode::Directory { entries } => entries,
                TmpFsNode::File { .. } => return Err(ErrorCode::ENOTDIR),
            };

            // Look up the entry
            *entries.get(name).ok_or(ErrorCode::ENOENT)?
        };

        // Check if it's a directory (we only allow unlinking files or empty directories)
        let node = self.nodes.get(&inode).ok_or(ErrorCode::ENOENT)?;
        match node {
            TmpFsNode::Directory { entries: dir_entries } => {
                // Only allow deleting empty directories
                if !dir_entries.is_empty() {
                    return Err(ErrorCode::ENOTEMPTY);
                }
            }
            TmpFsNode::File { .. } => {}
        }

        // Remove from parent
        let parent = self.nodes.get_mut(&parent_inode).ok_or(ErrorCode::ENOENT)?;
        match parent {
            TmpFsNode::Directory { entries } => {
                entries.remove(name);
            }
            TmpFsNode::File { .. } => return Err(ErrorCode::ENOTDIR),
        }

        // Remove the node
        self.nodes.remove(&inode);

        Ok(())
    }

    /// Read from a file
    pub fn read(&self, inode: Inode, offset: usize, buffer: &mut [u8]) -> Result<usize, ErrorCode> {
        let node = self.nodes.get(&inode).ok_or(ErrorCode::ENOENT)?;

        match node {
            TmpFsNode::File { data } => {
                if offset >= data.len() {
                    return Ok(0); // EOF
                }

                let available = data.len() - offset;
                let to_read = available.min(buffer.len());
                buffer[..to_read].copy_from_slice(&data[offset..offset + to_read]);
                Ok(to_read)
            }
            TmpFsNode::Directory { .. } => Err(ErrorCode::EISDIR),
        }
    }

    /// Write to a file
    pub fn write(&mut self, inode: Inode, offset: usize, data: &[u8]) -> Result<usize, ErrorCode> {
        let node = self.nodes.get_mut(&inode).ok_or(ErrorCode::ENOENT)?;

        match node {
            TmpFsNode::File { data: file_data } => {
                // Extend file if needed
                if offset > file_data.len() {
                    file_data.resize(offset, 0);
                }

                // Write data
                if offset == file_data.len() {
                    // Append
                    file_data.extend_from_slice(data);
                } else {
                    // Overwrite
                    let end_pos = offset + data.len();
                    if end_pos > file_data.len() {
                        file_data.resize(end_pos, 0);
                    }
                    file_data[offset..end_pos].copy_from_slice(data);
                }

                Ok(data.len())
            }
            TmpFsNode::Directory { .. } => Err(ErrorCode::EISDIR),
        }
    }

    /// Truncate a file to zero length
    pub fn truncate(&mut self, inode: Inode) -> Result<(), ErrorCode> {
        let node = self.nodes.get_mut(&inode).ok_or(ErrorCode::ENOENT)?;

        match node {
            TmpFsNode::File { data } => {
                data.clear();
                Ok(())
            }
            TmpFsNode::Directory { .. } => Err(ErrorCode::EISDIR),
        }
    }

    /// Get file metadata
    pub fn stat(&self, inode: Inode) -> Result<FileMetadata, ErrorCode> {
        let node = self.nodes.get(&inode).ok_or(ErrorCode::ENOENT)?;
        // Get actual mode from storage, or use default
        let mode = self.modes.get(&inode).copied().unwrap_or_else(|| match node.file_type() {
            FileType::Directory => crate::fs::DEFAULT_DIR_MODE,
            FileType::File => crate::fs::DEFAULT_FILE_MODE,
        });
        Ok(FileMetadata { file_type: node.file_type(), size: node.size(), mode })
    }

    /// Change file mode (chmod)
    pub fn chmod(&mut self, inode: Inode, new_mode: u16) -> Result<(), ErrorCode> {
        // Check that inode exists
        let node = self.nodes.get(&inode).ok_or(ErrorCode::ENOENT)?;

        // Preserve file type bits, only change permission bits
        let file_type_bits = match node.file_type() {
            FileType::Directory => crate::fs::S_IFDIR,
            FileType::File => crate::fs::S_IFREG,
        };
        let permission_bits = new_mode & 0o777;
        let final_mode = file_type_bits | permission_bits;

        // Store the new mode
        self.modes.insert(inode, final_mode);

        Ok(())
    }

    /// List directory entries
    pub fn read_dir(&self, inode: Inode) -> Result<Vec<(String, FileType)>, ErrorCode> {
        let node = self.nodes.get(&inode).ok_or(ErrorCode::ENOENT)?;

        match node {
            TmpFsNode::Directory { entries } => {
                let mut result = Vec::new();
                for (name, child_inode) in entries {
                    let child_node = self.nodes.get(child_inode).ok_or(ErrorCode::ENOENT)?;
                    result.push((name.clone(), child_node.file_type()));
                }
                Ok(result)
            }
            TmpFsNode::File { .. } => Err(ErrorCode::ENOTDIR),
        }
    }
}

/// Global tmpfs instance
static TMPFS: Mutex<Option<TmpFs>> = Mutex::new(None);

/// Initialize the tmpfs
pub fn init_tmpfs() {
    let mut tmpfs = TMPFS.lock();
    *tmpfs = Some(TmpFs::new());
}

/// Get a reference to the global tmpfs for operations
/// This is used internally by mount.rs to perform operations
pub fn with_tmpfs<F, R>(f: F) -> Result<R, ErrorCode>
where
    F: FnOnce(&mut TmpFs) -> Result<R, ErrorCode>,
{
    let mut tmpfs = TMPFS.lock();
    let fs = tmpfs.as_mut().ok_or(ErrorCode::EIO)?;
    f(fs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tmpfs_create_file() {
        let mut fs = TmpFs::new();
        let root = fs.root_inode();

        // Create a file in root
        let inode = fs.create(root, "test.txt", false).expect("create should succeed");
        assert!(inode > root);

        // File should be empty
        let metadata = fs.stat(inode).expect("stat should succeed");
        assert_eq!(metadata.file_type, FileType::File);
        assert_eq!(metadata.size, 0);
    }

    #[test]
    fn test_tmpfs_write_and_read() {
        let mut fs = TmpFs::new();
        let root = fs.root_inode();
        let inode = fs.create(root, "test.txt", false).unwrap();

        // Write data
        let data = b"hello world";
        let written = fs.write(inode, 0, data).expect("write should succeed");
        assert_eq!(written, data.len());

        // Read data back
        let mut buffer = [0u8; 64];
        let read = fs.read(inode, 0, &mut buffer).expect("read should succeed");
        assert_eq!(read, data.len());
        assert_eq!(&buffer[..read], data);

        // Check size
        let metadata = fs.stat(inode).unwrap();
        assert_eq!(metadata.size, data.len() as u64);
    }

    #[test]
    fn test_tmpfs_unlink() {
        let mut fs = TmpFs::new();
        let root = fs.root_inode();
        let _inode = fs.create(root, "test.txt", false).unwrap();

        // File should exist
        assert!(fs.lookup(root, "test.txt").is_ok());

        // Unlink the file
        fs.unlink(root, "test.txt").expect("unlink should succeed");

        // File should no longer exist
        assert_eq!(fs.lookup(root, "test.txt"), Err(ErrorCode::ENOENT));
    }

    #[test]
    fn test_tmpfs_directory() {
        let mut fs = TmpFs::new();
        let root = fs.root_inode();

        // Create a directory
        let dir_inode = fs.create(root, "subdir", true).unwrap();

        // Create a file in the directory
        let _file_inode = fs.create(dir_inode, "file.txt", false).unwrap();

        // List directory
        let entries = fs.read_dir(dir_inode).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "file.txt");
        assert_eq!(entries[0].1, FileType::File);

        // Lookup file
        assert!(fs.lookup(root, "subdir/file.txt").is_ok());
    }

    #[test]
    fn test_tmpfs_truncate() {
        let mut fs = TmpFs::new();
        let root = fs.root_inode();
        let inode = fs.create(root, "test.txt", false).unwrap();

        // Write data
        fs.write(inode, 0, b"hello world").unwrap();

        // Truncate
        fs.truncate(inode).expect("truncate should succeed");

        // Size should be 0
        let metadata = fs.stat(inode).unwrap();
        assert_eq!(metadata.size, 0);
    }
}
