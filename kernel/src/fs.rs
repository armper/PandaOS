//! Minimal in-memory filesystem for exec() and VFS reads.

use crate::pipe::PipeId;
use crate::syscall::ErrorCode;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

pub const MAX_FDS: usize = 16;
pub const FIRST_NONSTD_FD: usize = 3;

/// Open flags (subset of POSIX flags)
pub const O_RDONLY: u64 = 0x0000;
pub const O_WRONLY: u64 = 0x0001;
pub const O_RDWR: u64 = 0x0002;
pub const O_CREAT: u64 = 0x0040;
pub const O_TRUNC: u64 = 0x0200;
pub const O_APPEND: u64 = 0x0400;

/// Global storage for writable files in /tmp
/// Maps node_index to file contents
static WRITABLE_FILES: Mutex<Option<BTreeMap<usize, Vec<u8>>>> = Mutex::new(None);

/// Dynamic file registry for files created at runtime in /tmp
/// Maps path to node index
static DYNAMIC_FILES: Mutex<Option<BTreeMap<String, usize>>> = Mutex::new(None);

/// Mode storage for writable files (in-memory and dynamic)
/// Maps node_index to mode bits
static FILE_MODES: Mutex<Option<BTreeMap<usize, u16>>> = Mutex::new(None);

/// Ownership storage for files (in-memory and dynamic)
/// Maps node_index to (uid, gid) tuple
static FILE_OWNERSHIP: Mutex<Option<BTreeMap<usize, (u32, u32)>>> = Mutex::new(None);

/// File type enumeration
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FileType {
    /// Regular file
    File = 0,
    /// Directory
    Directory = 1,
}

// POSIX mode bit constants
/// Directory type bit (octal 040000)
pub const S_IFDIR: u16 = 0o040000;
/// Regular file type bit (octal 0100000)
pub const S_IFREG: u16 = 0o100000;
/// Type mask
pub const S_IFMT: u16 = 0o170000;

/// Permission bits
pub const S_IRWXU: u16 = 0o0700; // User rwx
pub const S_IRUSR: u16 = 0o0400; // User read
pub const S_IWUSR: u16 = 0o0200; // User write
pub const S_IXUSR: u16 = 0o0100; // User execute
pub const S_IRWXG: u16 = 0o0070; // Group rwx
pub const S_IRGRP: u16 = 0o0040; // Group read
pub const S_IWGRP: u16 = 0o0020; // Group write
pub const S_IXGRP: u16 = 0o0010; // Group execute
pub const S_IRWXO: u16 = 0o0007; // Other rwx
pub const S_IROTH: u16 = 0o0004; // Other read
pub const S_IWOTH: u16 = 0o0002; // Other write
pub const S_IXOTH: u16 = 0o0001; // Other execute

/// Default mode for directories: 040755 (drwxr-xr-x)
pub const DEFAULT_DIR_MODE: u16 = S_IFDIR | 0o755;
/// Default mode for regular files: 0100644 (-rw-r--r--)
pub const DEFAULT_FILE_MODE: u16 = S_IFREG | 0o644;

/// File metadata returned by stat/fstat
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileMetadata {
    /// File type (File or Directory)
    pub file_type: FileType,
    /// Size in bytes (0 for directories)
    pub size: u64,
    /// POSIX mode (file type + permission bits)
    pub mode: u16,
    /// Owner user ID
    pub uid: u32,
    /// Owner group ID
    pub gid: u32,
}

impl FileMetadata {
    /// Check if this is a directory
    pub fn is_dir(&self) -> bool {
        self.file_type == FileType::Directory
    }

    /// Check if this is a regular file
    pub fn is_file(&self) -> bool {
        self.file_type == FileType::File
    }
}

/// Permission check helpers following Unix owner/group/other semantics

/// Check if a process can read a file/directory
/// Follows Unix semantics: checks owner, then group, then other permissions
pub fn can_read(proc_uid: u32, proc_gid: u32, file_uid: u32, file_gid: u32, mode: u16) -> bool {
    if proc_uid == 0 {
        // Root can read anything with any read permission
        (mode & (S_IRUSR | S_IRGRP | S_IROTH)) != 0
    } else if proc_uid == file_uid {
        // Owner permissions
        (mode & S_IRUSR) != 0
    } else if proc_gid == file_gid {
        // Group permissions
        (mode & S_IRGRP) != 0
    } else {
        // Other permissions
        (mode & S_IROTH) != 0
    }
}

/// Check if a process can write to a file/directory
pub fn can_write(proc_uid: u32, proc_gid: u32, file_uid: u32, file_gid: u32, mode: u16) -> bool {
    if proc_uid == 0 {
        // Root can write to anything with any write permission
        (mode & (S_IWUSR | S_IWGRP | S_IWOTH)) != 0
    } else if proc_uid == file_uid {
        // Owner permissions
        (mode & S_IWUSR) != 0
    } else if proc_gid == file_gid {
        // Group permissions
        (mode & S_IWGRP) != 0
    } else {
        // Other permissions
        (mode & S_IWOTH) != 0
    }
}

/// Check if a file is executable by a process
pub fn can_exec(proc_uid: u32, proc_gid: u32, file_uid: u32, file_gid: u32, mode: u16) -> bool {
    if proc_uid == 0 {
        // Root can exec anything with any exec permission
        (mode & (S_IXUSR | S_IXGRP | S_IXOTH)) != 0
    } else if proc_uid == file_uid {
        // Owner permissions
        (mode & S_IXUSR) != 0
    } else if proc_gid == file_gid {
        // Group permissions
        (mode & S_IXGRP) != 0
    } else {
        // Other permissions
        (mode & S_IXOTH) != 0
    }
}

/// Check if a directory can be traversed (x permission) by a process
pub fn can_traverse(proc_uid: u32, proc_gid: u32, file_uid: u32, file_gid: u32, mode: u16) -> bool {
    can_exec(proc_uid, proc_gid, file_uid, file_gid, mode)
}

/// Check if a directory can be listed (r permission) by a process
pub fn can_list(proc_uid: u32, proc_gid: u32, file_uid: u32, file_gid: u32, mode: u16) -> bool {
    can_read(proc_uid, proc_gid, file_uid, file_gid, mode)
}

pub struct FileNode {
    pub path: &'static str,
    pub data: &'static [u8],
    pub file_type: FileType,
    pub mode: u16,
    pub uid: u32,
    pub gid: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct OpenFile {
    pub node_index: usize,
    pub offset: usize,
    pub flags: u64,
}

/// Open disk file descriptor
#[derive(Clone, Copy, Debug)]
pub struct OpenDiskFile {
    pub inode: u32,
    pub offset: usize,
    pub flags: u64,
}

/// Open tmpfs file descriptor
#[derive(Clone, Copy, Debug)]
pub struct OpenTmpfsFile {
    pub inode: u32,
    pub offset: usize,
    pub flags: u64,
}

/// File descriptor kinds
#[derive(Clone, Copy, Debug)]
pub enum FdKind {
    /// Regular file (with read/write flags)
    File(OpenFile, bool), // (open_file, writable)
    /// Directory (opened for reading entries)
    Directory(OpenFile),
    /// Disk file from mounted filesystem
    DiskFile(OpenDiskFile),
    /// Disk directory from mounted filesystem
    DiskDirectory(OpenDiskFile),
    /// Tmpfs file from tmpfs filesystem
    TmpfsFile(OpenTmpfsFile),
    /// Tmpfs directory from tmpfs filesystem
    TmpfsDirectory(OpenTmpfsFile),
    /// Pipe read end
    PipeRead(PipeId),
    /// Pipe write end
    PipeWrite(PipeId),
}

#[derive(Clone, Copy, Debug)]
pub struct FdTable {
    entries: [Option<FdKind>; MAX_FDS],
}

impl FdTable {
    pub const fn new() -> Self {
        Self { entries: [None; MAX_FDS] }
    }

    /// Allocate the lowest available fd >= 3
    fn allocate_fd(&mut self) -> Result<usize, ErrorCode> {
        for fd in FIRST_NONSTD_FD..MAX_FDS {
            if self.entries[fd].is_none() {
                return Ok(fd);
            }
        }
        Err(ErrorCode::EMFILE)
    }

    pub fn open_node(
        &mut self,
        node_index: usize,
        proc_uid: u32,
        proc_gid: u32,
    ) -> Result<i32, ErrorCode> {
        self.open_node_with_flags(node_index, O_RDONLY, proc_uid, proc_gid)
    }

    pub fn open_node_with_flags(
        &mut self,
        node_index: usize,
        flags: u64,
        proc_uid: u32,
        proc_gid: u32,
    ) -> Result<i32, ErrorCode> {
        let node = FILES.get(node_index).ok_or(ErrorCode::ENOENT)?;
        let fd = self.allocate_fd()?;

        // Get file metadata to check permissions
        let metadata = stat_path(node.path)?;

        // Check access mode
        // O_RDONLY=0, O_WRONLY=1, O_RDWR=2
        // Writable if O_WRONLY or O_RDWR is set
        let writable = (flags & O_WRONLY) != 0 || (flags & O_RDWR) != 0;
        // Readable if not O_WRONLY (covers O_RDONLY and O_RDWR)
        // We need both conditions because O_RDWR (2) & O_WRONLY (1) == 0
        let readable = (flags & O_WRONLY) == 0 || (flags & O_RDWR) != 0;

        // Check permissions
        if readable && !can_read(proc_uid, proc_gid, metadata.uid, metadata.gid, metadata.mode) {
            return Err(ErrorCode::EACCES);
        }
        if writable && !can_write(proc_uid, proc_gid, metadata.uid, metadata.gid, metadata.mode) {
            return Err(ErrorCode::EACCES);
        }

        match node.file_type {
            FileType::File => {
                // Handle O_TRUNC flag for writable files
                if writable && (flags & O_TRUNC) != 0 {
                    // Truncate file
                    let mut files = WRITABLE_FILES.lock();
                    let store = files.get_or_insert_with(BTreeMap::new);
                    store.insert(node_index, Vec::new());
                }

                self.entries[fd] =
                    Some(FdKind::File(OpenFile { node_index, offset: 0, flags }, writable));
            }
            FileType::Directory => {
                if writable {
                    return Err(ErrorCode::EISDIR);
                }
                self.entries[fd] =
                    Some(FdKind::Directory(OpenFile { node_index, offset: 0, flags }));
            }
        }
        Ok(fd as i32)
    }

    /// Open a pipe read end
    pub fn open_pipe_read(&mut self, pipe_id: PipeId) -> Result<i32, ErrorCode> {
        let fd = self.allocate_fd()?;
        self.entries[fd] = Some(FdKind::PipeRead(pipe_id));
        Ok(fd as i32)
    }

    /// Open a pipe write end
    pub fn open_pipe_write(&mut self, pipe_id: PipeId) -> Result<i32, ErrorCode> {
        let fd = self.allocate_fd()?;
        self.entries[fd] = Some(FdKind::PipeWrite(pipe_id));
        Ok(fd as i32)
    }

    /// Get the fd kind
    pub fn get(&self, fd: i32) -> Result<FdKind, ErrorCode> {
        if fd < 0 || fd as usize >= MAX_FDS {
            return Err(ErrorCode::EBADF);
        }
        if fd < FIRST_NONSTD_FD as i32 {
            return Err(ErrorCode::EBADF);
        }
        self.entries[fd as usize].ok_or(ErrorCode::EBADF)
    }

    pub fn close(&mut self, fd: i32) -> Result<(), ErrorCode> {
        if fd < 0 || fd as usize >= MAX_FDS {
            return Err(ErrorCode::EBADF);
        }
        if fd < FIRST_NONSTD_FD as i32 {
            return Err(ErrorCode::EINVAL);
        }
        let slot = &mut self.entries[fd as usize];
        if let Some(kind) = slot.take() {
            // Close pipe ends if needed
            match kind {
                FdKind::PipeRead(pipe_id) => {
                    crate::pipe::pipe_close_read(pipe_id)?;
                }
                FdKind::PipeWrite(pipe_id) => {
                    crate::pipe::pipe_close_write(pipe_id)?;
                }
                FdKind::File(_, _)
                | FdKind::Directory(_)
                | FdKind::DiskFile(_)
                | FdKind::DiskDirectory(_)
                | FdKind::TmpfsFile(_)
                | FdKind::TmpfsDirectory(_) => {
                    // Files and directories don't need cleanup
                }
            }
            Ok(())
        } else {
            Err(ErrorCode::EBADF)
        }
    }

    /// Read from a file descriptor into a buffer
    /// Returns the number of bytes read
    pub fn read(&mut self, fd: i32, buffer: &mut [u8]) -> Result<usize, ErrorCode> {
        if fd < 0 || fd as usize >= MAX_FDS {
            return Err(ErrorCode::EBADF);
        }
        if fd < FIRST_NONSTD_FD as i32 {
            return Err(ErrorCode::EBADF);
        }
        let kind = self.entries[fd as usize].ok_or(ErrorCode::EBADF)?;
        match kind {
            FdKind::File(open, _writable) => {
                let count = buffer.len();

                // Check if this is a writable file with data in WRITABLE_FILES
                let files = WRITABLE_FILES.lock();
                if let Some(ref store) = *files {
                    if let Some(file_data) = store.get(&open.node_index) {
                        // Read from writable file storage
                        if open.offset >= file_data.len() || count == 0 {
                            return Ok(0);
                        }
                        let available = file_data.len() - open.offset;
                        let to_read = available.min(count);
                        let start = open.offset;
                        let end = start + to_read;

                        // Copy data to buffer
                        buffer[..to_read].copy_from_slice(&file_data[start..end]);

                        // Update offset
                        drop(files);
                        self.entries[fd as usize] = Some(FdKind::File(
                            OpenFile {
                                node_index: open.node_index,
                                offset: end,
                                flags: open.flags,
                            },
                            _writable,
                        ));

                        return Ok(to_read);
                    }
                }
                drop(files);

                // Fall back to static file data
                let node = FILES.get(open.node_index).ok_or(ErrorCode::ENOENT)?;
                if open.offset >= node.data.len() || count == 0 {
                    return Ok(0);
                }
                let available = node.data.len() - open.offset;
                let to_read = available.min(count);
                let start = open.offset;
                let end = start + to_read;

                // Copy data to buffer
                buffer[..to_read].copy_from_slice(&node.data[start..end]);

                self.entries[fd as usize] = Some(FdKind::File(
                    OpenFile { node_index: open.node_index, offset: end, flags: open.flags },
                    _writable,
                ));
                Ok(to_read)
            }
            FdKind::Directory(_) => {
                // Reading directories via read() is not supported
                // Use getdents64 syscall instead
                Err(ErrorCode::EISDIR)
            }
            FdKind::DiskFile(open) => {
                // Read from disk file
                let bytes_read = crate::mount::diskfs_read(open.inode, open.offset, buffer)?;

                // Update offset
                let new_offset = open.offset + bytes_read;
                self.entries[fd as usize] = Some(FdKind::DiskFile(OpenDiskFile {
                    inode: open.inode,
                    offset: new_offset,
                    flags: open.flags,
                }));

                Ok(bytes_read)
            }
            FdKind::DiskDirectory(_) => {
                // Reading directories via read() is not supported
                Err(ErrorCode::EISDIR)
            }
            FdKind::TmpfsFile(open) => {
                // Read from tmpfs file
                let mut buffer_vec = alloc::vec![0u8; buffer.len()];
                let bytes_read =
                    crate::mount::tmpfs_read(open.inode, open.offset, &mut buffer_vec)?;
                buffer[..bytes_read].copy_from_slice(&buffer_vec[..bytes_read]);

                // Update offset
                let new_offset = open.offset + bytes_read;
                self.entries[fd as usize] = Some(FdKind::TmpfsFile(OpenTmpfsFile {
                    inode: open.inode,
                    offset: new_offset,
                    flags: open.flags,
                }));

                Ok(bytes_read)
            }
            FdKind::TmpfsDirectory(_) => {
                // Reading directories via read() is not supported
                Err(ErrorCode::EISDIR)
            }
            FdKind::PipeRead(_) | FdKind::PipeWrite(_) => {
                // Pipes are handled through separate syscall path
                Err(ErrorCode::EBADF)
            }
        }
    }

    /// Write to a file descriptor
    pub fn write(&mut self, fd: i32, data: &[u8]) -> Result<usize, ErrorCode> {
        if fd < 0 || fd as usize >= MAX_FDS {
            return Err(ErrorCode::EBADF);
        }
        if fd < FIRST_NONSTD_FD as i32 {
            return Err(ErrorCode::EBADF);
        }

        let kind = self.entries[fd as usize].ok_or(ErrorCode::EBADF)?;
        match kind {
            FdKind::File(open, writable) => {
                if !writable {
                    return Err(ErrorCode::EBADF);
                }

                // Check if O_APPEND is set
                let write_offset = if (open.flags & O_APPEND) != 0 {
                    // Get current file size for append
                    let files = WRITABLE_FILES.lock();
                    let current_size = if let Some(ref store) = *files {
                        store.get(&open.node_index).map(|v| v.len()).unwrap_or(0)
                    } else {
                        0
                    };
                    drop(files);
                    current_size
                } else {
                    open.offset
                };

                // Write to writable file storage
                let mut files = WRITABLE_FILES.lock();
                let store = files.get_or_insert_with(BTreeMap::new);
                let file_data = store.entry(open.node_index).or_insert_with(Vec::new);

                // Ensure file is large enough for the write at current offset
                if write_offset > file_data.len() {
                    file_data.resize(write_offset, 0);
                }

                // Write or overwrite data at offset
                if write_offset == file_data.len() {
                    // Append
                    file_data.extend_from_slice(data);
                } else {
                    // Overwrite - extend file if needed
                    let end_pos = write_offset + data.len();
                    if end_pos > file_data.len() {
                        file_data.resize(end_pos, 0);
                    }
                    file_data[write_offset..end_pos].copy_from_slice(data);
                }

                // Update offset
                let new_offset = write_offset + data.len();
                self.entries[fd as usize] = Some(FdKind::File(
                    OpenFile { node_index: open.node_index, offset: new_offset, flags: open.flags },
                    writable,
                ));

                Ok(data.len())
            }
            FdKind::Directory(_) => Err(ErrorCode::EBADF),
            FdKind::DiskFile(_) | FdKind::DiskDirectory(_) => Err(ErrorCode::EROFS),
            FdKind::TmpfsFile(open) => {
                // Check if O_APPEND is set
                let write_offset = if (open.flags & O_APPEND) != 0 {
                    // Get current file size for append
                    let metadata = crate::mount::tmpfs_stat(open.inode)?;
                    metadata.size as usize
                } else {
                    open.offset
                };

                // Write to tmpfs file
                let bytes_written = crate::mount::tmpfs_write(open.inode, write_offset, data)?;

                // Update offset
                let new_offset = write_offset + bytes_written;
                self.entries[fd as usize] = Some(FdKind::TmpfsFile(OpenTmpfsFile {
                    inode: open.inode,
                    offset: new_offset,
                    flags: open.flags,
                }));

                Ok(bytes_written)
            }
            FdKind::TmpfsDirectory(_) => Err(ErrorCode::EBADF),
            FdKind::PipeRead(_) | FdKind::PipeWrite(_) => {
                // Pipes are handled through separate syscall path
                Err(ErrorCode::EBADF)
            }
        }
    }

    /// Update directory offset (for getdents64)
    pub fn update_directory_offset(&mut self, fd: i32, new_offset: usize) -> Result<(), ErrorCode> {
        if fd < 0 || fd as usize >= MAX_FDS {
            return Err(ErrorCode::EBADF);
        }
        if fd < FIRST_NONSTD_FD as i32 {
            return Err(ErrorCode::EBADF);
        }

        let kind = self.entries[fd as usize].ok_or(ErrorCode::EBADF)?;
        match kind {
            FdKind::Directory(open) => {
                self.entries[fd as usize] = Some(FdKind::Directory(OpenFile {
                    node_index: open.node_index,
                    offset: new_offset,
                    flags: open.flags,
                }));
                Ok(())
            }
            FdKind::DiskDirectory(open) => {
                self.entries[fd as usize] = Some(FdKind::DiskDirectory(OpenDiskFile {
                    inode: open.inode,
                    offset: new_offset,
                    flags: open.flags,
                }));
                Ok(())
            }
            FdKind::TmpfsDirectory(open) => {
                self.entries[fd as usize] = Some(FdKind::TmpfsDirectory(OpenTmpfsFile {
                    inode: open.inode,
                    offset: new_offset,
                    flags: open.flags,
                }));
                Ok(())
            }
            _ => Err(ErrorCode::ENOTDIR),
        }
    }

    /// Duplicate fd (for dup2)
    pub fn dup2(&mut self, oldfd: i32, newfd: i32) -> Result<(), ErrorCode> {
        if oldfd < 0 || oldfd as usize >= MAX_FDS {
            return Err(ErrorCode::EBADF);
        }
        if newfd < 0 || newfd as usize >= MAX_FDS {
            return Err(ErrorCode::EBADF);
        }

        // Can't dup FROM stdin/stdout/stderr, but can dup TO them for pipe redirection
        if oldfd < FIRST_NONSTD_FD as i32 {
            return Err(ErrorCode::EINVAL);
        }

        let old_kind = self.entries[oldfd as usize].ok_or(ErrorCode::EBADF)?;

        // Close newfd if it's open and it's not a standard fd
        if newfd >= FIRST_NONSTD_FD as i32 && self.entries[newfd as usize].is_some() {
            self.close(newfd)?;
        }

        // Duplicate the fd kind and increment refcounts for pipes
        match old_kind {
            FdKind::File(open, writable) => {
                self.entries[newfd as usize] = Some(FdKind::File(open, writable));
            }
            FdKind::Directory(open) => {
                self.entries[newfd as usize] = Some(FdKind::Directory(open));
            }
            FdKind::DiskFile(open) => {
                self.entries[newfd as usize] = Some(FdKind::DiskFile(open));
            }
            FdKind::DiskDirectory(open) => {
                self.entries[newfd as usize] = Some(FdKind::DiskDirectory(open));
            }
            FdKind::TmpfsFile(open) => {
                self.entries[newfd as usize] = Some(FdKind::TmpfsFile(open));
            }
            FdKind::TmpfsDirectory(open) => {
                self.entries[newfd as usize] = Some(FdKind::TmpfsDirectory(open));
            }
            FdKind::PipeRead(pipe_id) => {
                crate::pipe::pipe_open_read_end(pipe_id)?;
                self.entries[newfd as usize] = Some(FdKind::PipeRead(pipe_id));
            }
            FdKind::PipeWrite(pipe_id) => {
                crate::pipe::pipe_open_write_end(pipe_id)?;
                self.entries[newfd as usize] = Some(FdKind::PipeWrite(pipe_id));
            }
        }

        Ok(())
    }

    /// Fork the FD table (for use in fork())
    /// Increments refcounts for all pipe fds
    pub fn fork_copy(&self) -> Result<Self, ErrorCode> {
        let new_table = *self;

        // Increment refcounts for all pipe fds
        for entry in &new_table.entries {
            if let Some(kind) = entry {
                match kind {
                    FdKind::PipeRead(pipe_id) => {
                        crate::pipe::pipe_open_read_end(*pipe_id)?;
                    }
                    FdKind::PipeWrite(pipe_id) => {
                        crate::pipe::pipe_open_write_end(*pipe_id)?;
                    }
                    FdKind::File(_, _)
                    | FdKind::Directory(_)
                    | FdKind::DiskFile(_)
                    | FdKind::DiskDirectory(_)
                    | FdKind::TmpfsFile(_)
                    | FdKind::TmpfsDirectory(_) => {
                        // Files and directories don't need refcounting
                    }
                }
            }
        }

        Ok(new_table)
    }

    /// Get the current offset for a file descriptor
    pub fn get_offset(&self, fd: i32) -> Result<i64, ErrorCode> {
        if fd < 0 || fd as usize >= MAX_FDS {
            return Err(ErrorCode::EBADF);
        }
        if fd < FIRST_NONSTD_FD as i32 {
            return Err(ErrorCode::EBADF);
        }

        let kind = self.entries[fd as usize].ok_or(ErrorCode::EBADF)?;
        match kind {
            FdKind::File(open, _) => Ok(open.offset as i64),
            FdKind::DiskFile(open) => Ok(open.offset as i64),
            FdKind::TmpfsFile(open) => Ok(open.offset as i64),
            FdKind::Directory(_) | FdKind::DiskDirectory(_) | FdKind::TmpfsDirectory(_) => {
                Err(ErrorCode::EISDIR)
            }
            FdKind::PipeRead(_) | FdKind::PipeWrite(_) => Err(ErrorCode::ESPIPE),
        }
    }

    /// Set the offset for a file descriptor (lseek)
    pub fn set_offset(&mut self, fd: i32, new_offset: i64) -> Result<i64, ErrorCode> {
        if fd < 0 || fd as usize >= MAX_FDS {
            return Err(ErrorCode::EBADF);
        }
        if fd < FIRST_NONSTD_FD as i32 {
            return Err(ErrorCode::EBADF);
        }
        if new_offset < 0 {
            return Err(ErrorCode::EINVAL);
        }

        let kind = self.entries[fd as usize].ok_or(ErrorCode::EBADF)?;
        match kind {
            FdKind::File(open, writable) => {
                let new_offset = new_offset as usize;
                self.entries[fd as usize] = Some(FdKind::File(
                    OpenFile { node_index: open.node_index, offset: new_offset, flags: open.flags },
                    writable,
                ));
                Ok(new_offset as i64)
            }
            FdKind::DiskFile(open) => {
                let new_offset = new_offset as usize;
                self.entries[fd as usize] = Some(FdKind::DiskFile(OpenDiskFile {
                    inode: open.inode,
                    offset: new_offset,
                    flags: open.flags,
                }));
                Ok(new_offset as i64)
            }
            FdKind::TmpfsFile(open) => {
                let new_offset = new_offset as usize;
                self.entries[fd as usize] = Some(FdKind::TmpfsFile(OpenTmpfsFile {
                    inode: open.inode,
                    offset: new_offset,
                    flags: open.flags,
                }));
                Ok(new_offset as i64)
            }
            FdKind::Directory(_) | FdKind::DiskDirectory(_) | FdKind::TmpfsDirectory(_) => {
                Err(ErrorCode::EISDIR)
            }
            FdKind::PipeRead(_) | FdKind::PipeWrite(_) => Err(ErrorCode::ESPIPE),
        }
    }

    /// Get file size for lseek `SEEK_END`
    pub fn get_file_size(&self, fd: i32) -> Result<i64, ErrorCode> {
        if fd < 0 || fd as usize >= MAX_FDS {
            return Err(ErrorCode::EBADF);
        }
        if fd < FIRST_NONSTD_FD as i32 {
            return Err(ErrorCode::EBADF);
        }

        let kind = self.entries[fd as usize].ok_or(ErrorCode::EBADF)?;
        match kind {
            FdKind::File(open, _) => {
                let node = FILES.get(open.node_index).ok_or(ErrorCode::ENOENT)?;
                // Check writable files storage
                let files = WRITABLE_FILES.lock();
                if let Some(ref store) = *files {
                    if let Some(file_data) = store.get(&open.node_index) {
                        return Ok(file_data.len() as i64);
                    }
                }
                Ok(node.data.len() as i64)
            }
            FdKind::DiskFile(open) => {
                let metadata = crate::mount::diskfs_stat(open.inode)?;
                Ok(metadata.size as i64)
            }
            FdKind::TmpfsFile(open) => {
                let metadata = crate::mount::tmpfs_stat(open.inode)?;
                Ok(metadata.size as i64)
            }
            FdKind::Directory(_) | FdKind::DiskDirectory(_) | FdKind::TmpfsDirectory(_) => {
                Err(ErrorCode::EISDIR)
            }
            FdKind::PipeRead(_) | FdKind::PipeWrite(_) => Err(ErrorCode::ESPIPE),
        }
    }
}

pub static FILES: &[FileNode] = &[
    // Root directory
    FileNode {
        path: "/",
        data: b"",
        file_type: FileType::Directory,
        mode: DEFAULT_DIR_MODE,
        uid: 0,
        gid: 0,
    },
    // /bin directory (now empty - programs loaded from disk/tmpfs)
    FileNode {
        path: "/bin",
        data: b"",
        file_type: FileType::Directory,
        mode: DEFAULT_DIR_MODE,
        uid: 0,
        gid: 0,
    },
    // /etc directory
    FileNode {
        path: "/etc",
        data: b"",
        file_type: FileType::Directory,
        mode: DEFAULT_DIR_MODE,
        uid: 0,
        gid: 0,
    },
    // /tmp directory (writable)
    FileNode {
        path: "/tmp",
        data: b"",
        file_type: FileType::Directory,
        mode: DEFAULT_DIR_MODE,
        uid: 0,
        gid: 0,
    },
    // /mnt directory (mount point for disk filesystem)
    FileNode {
        path: "/mnt",
        data: b"",
        file_type: FileType::Directory,
        mode: DEFAULT_DIR_MODE,
        uid: 0,
        gid: 0,
    },
    // Configuration files
    FileNode {
        path: "/etc/motd",
        data: b"Welcome to PandaOS.\r\nType 'help' for commands.\r\n",
        file_type: FileType::File,
        mode: DEFAULT_FILE_MODE,
        uid: 0,
        gid: 0,
    },
    FileNode {
        path: "/etc/version",
        data: b"PandaOS 0.1.0\r\n",
        file_type: FileType::File,
        mode: DEFAULT_FILE_MODE,
        uid: 0,
        gid: 0,
    },
];

/// Look up a file by absolute path.
pub fn lookup(path: &str) -> Option<&'static [u8]> {
    FILES.iter().find(|entry| entry.path == path).map(|entry| entry.data)
}

/// Look up a file node by absolute path (including dynamic files)
pub fn lookup_node(path: &str) -> Option<(usize, &'static FileNode)> {
    // First check static FILES
    if let Some(found) = FILES.iter().enumerate().find(|(_, entry)| entry.path == path) {
        return Some(found);
    }

    // Check dynamic files created in /tmp
    let dynamic = DYNAMIC_FILES.lock();
    if let Some(ref map) = *dynamic {
        if let Some(&node_index) = map.get(path) {
            // Return a synthetic node - we'll use a placeholder from FILES
            // The actual data is in WRITABLE_FILES
            return Some((node_index, &FILES[0])); // Placeholder
        }
    }

    None
}

/// Create a new dynamic file in /tmp
fn create_dynamic_file(path: &str) -> Result<usize, ErrorCode> {
    // Validate path is in /tmp
    if !path.starts_with("/tmp/") {
        return Err(ErrorCode::EACCES);
    }

    // Check if file already exists
    if lookup_node(path).is_some() {
        return Err(ErrorCode::EEXIST);
    }

    // Allocate a new node index (use high numbers to avoid conflicts)
    // We'll use FILES.len() + dynamic file count
    let mut dynamic = DYNAMIC_FILES.lock();
    let map = dynamic.get_or_insert_with(BTreeMap::new);

    let node_index = FILES.len() + map.len();
    map.insert(String::from(path), node_index);

    // Initialize empty file in writable storage
    let mut files = WRITABLE_FILES.lock();
    let store = files.get_or_insert_with(BTreeMap::new);
    store.insert(node_index, Vec::new());

    Ok(node_index)
}

/// Open a file by path into a file descriptor table with flags.
pub fn open_path_with_flags(
    table: &mut FdTable,
    path: &str,
    flags: u64,
    proc_uid: u32,
    proc_gid: u32,
) -> Result<i32, ErrorCode> {
    // Check if path is on a mounted filesystem
    if let Some((_mount, rel_path, fs_type)) = crate::mount::resolve_mount_path(path) {
        match fs_type {
            crate::mount::FsType::Disk => {
                // Open file from disk filesystem
                let inode = crate::mount::diskfs_lookup(&rel_path)?;
                let metadata = crate::mount::diskfs_stat(inode)?;

                // Check access mode and permissions
                // O_RDONLY=0, O_WRONLY=1, O_RDWR=2
                let writable = (flags & O_WRONLY) != 0 || (flags & O_RDWR) != 0;
                // Readable if not O_WRONLY (covers O_RDONLY and O_RDWR)
                let readable = (flags & O_WRONLY) == 0 || (flags & O_RDWR) != 0;

                // Check permissions
                if readable
                    && !can_read(proc_uid, proc_gid, metadata.uid, metadata.gid, metadata.mode)
                {
                    return Err(ErrorCode::EACCES);
                }
                if writable {
                    // Check write permission
                    if !can_write(proc_uid, proc_gid, metadata.uid, metadata.gid, metadata.mode) {
                        return Err(ErrorCode::EACCES);
                    }
                    // Disk fs is read-only
                    return Err(ErrorCode::EROFS);
                }

                // Allocate FD
                let fd = table.allocate_fd()?;

                // Open based on file type
                match metadata.file_type {
                    FileType::File => {
                        table.entries[fd] =
                            Some(FdKind::DiskFile(OpenDiskFile { inode, offset: 0, flags }));
                    }
                    FileType::Directory => {
                        table.entries[fd] =
                            Some(FdKind::DiskDirectory(OpenDiskFile { inode, offset: 0, flags }));
                    }
                }

                return Ok(fd as i32);
            }
            crate::mount::FsType::Tmpfs => {
                // Try to lookup file in tmpfs
                let inode_result = crate::mount::tmpfs_lookup(&rel_path);

                let inode = match inode_result {
                    Ok(inode) => {
                        // File exists
                        // If O_TRUNC is set and writable, truncate it
                        if (flags & O_TRUNC) != 0
                            && ((flags & O_WRONLY) != 0 || (flags & O_RDWR) != 0)
                        {
                            crate::mount::tmpfs_truncate(inode)?;
                        }
                        inode
                    }
                    Err(ErrorCode::ENOENT) => {
                        // File doesn't exist - check if O_CREAT is set
                        if (flags & O_CREAT) != 0 {
                            // Extract parent path and filename
                            let (parent, name) = if let Some(pos) = rel_path.rfind('/') {
                                let parent = &rel_path[..pos];
                                let name = &rel_path[pos + 1..];
                                (if parent.is_empty() { "/" } else { parent }, name)
                            } else {
                                ("/", rel_path.as_str())
                            };

                            // Create the file
                            crate::mount::tmpfs_create(parent, name, false)?
                        } else {
                            return Err(ErrorCode::ENOENT);
                        }
                    }
                    Err(e) => return Err(e),
                };

                // Get metadata
                let metadata = crate::mount::tmpfs_stat(inode)?;

                // Check access mode and permissions
                // O_RDONLY=0, O_WRONLY=1, O_RDWR=2
                let writable = (flags & O_WRONLY) != 0 || (flags & O_RDWR) != 0;
                // Readable if not O_WRONLY (covers O_RDONLY and O_RDWR)
                let readable = (flags & O_WRONLY) == 0 || (flags & O_RDWR) != 0;

                // Check permissions
                if readable
                    && !can_read(proc_uid, proc_gid, metadata.uid, metadata.gid, metadata.mode)
                {
                    return Err(ErrorCode::EACCES);
                }
                if writable
                    && !can_write(proc_uid, proc_gid, metadata.uid, metadata.gid, metadata.mode)
                {
                    return Err(ErrorCode::EACCES);
                }

                // Allocate FD
                let fd = table.allocate_fd()?;

                // Open based on file type
                match metadata.file_type {
                    FileType::File => {
                        table.entries[fd] =
                            Some(FdKind::TmpfsFile(OpenTmpfsFile { inode, offset: 0, flags }));
                    }
                    FileType::Directory => {
                        table.entries[fd] =
                            Some(FdKind::TmpfsDirectory(OpenTmpfsFile { inode, offset: 0, flags }));
                    }
                }

                return Ok(fd as i32);
            }
        }
    }

    // Try to look up existing file in memory filesystem
    let node_index = if let Some((idx, _node)) = lookup_node(path) {
        idx
    } else {
        // File doesn't exist - check if O_CREAT is set
        if (flags & O_CREAT) != 0 {
            create_dynamic_file(path)?
        } else {
            return Err(ErrorCode::ENOENT);
        }
    };

    table.open_node_with_flags(node_index, flags, proc_uid, proc_gid)
}

/// Open a file by path into a file descriptor table.
pub fn open_path(
    table: &mut FdTable,
    path: &str,
    proc_uid: u32,
    proc_gid: u32,
) -> Result<i32, ErrorCode> {
    let (node_index, _node) = lookup_node(path).ok_or(ErrorCode::ENOENT)?;
    table.open_node(node_index, proc_uid, proc_gid)
}

/// Directory entry for getdents64 syscall
#[repr(C, packed)]
pub struct DirEntry {
    /// Inode number (we use node_index)
    pub d_ino: u64,
    /// Offset to next entry
    pub d_off: u64,
    /// Length of this record
    pub d_reclen: u16,
    /// File type
    pub d_type: u8,
    // Null-terminated filename (variable length) followed by padding to align to 8 bytes
}

/// List directory entries for a given directory path
/// Returns a list of (name, file_type) tuples
pub fn list_directory(
    dir_path: &str,
) -> Result<alloc::vec::Vec<(alloc::string::String, FileType)>, ErrorCode> {
    use alloc::string::String;
    use alloc::vec::Vec;

    // Check if path is on a mounted filesystem
    if let Some((_mount, rel_path, fs_type)) = crate::mount::resolve_mount_path(dir_path) {
        match fs_type {
            crate::mount::FsType::Disk => {
                // List directory from disk filesystem
                let inode = crate::mount::diskfs_lookup(&rel_path)?;
                return crate::mount::diskfs_list_dir(inode);
            }
            crate::mount::FsType::Tmpfs => {
                // List directory from tmpfs
                let inode = crate::mount::tmpfs_lookup(&rel_path)?;
                return crate::mount::tmpfs_list_dir(inode);
            }
        }
    }

    // Verify the path is a directory
    let (_, node) = lookup_node(dir_path).ok_or(ErrorCode::ENOENT)?;
    if node.file_type != FileType::Directory {
        return Err(ErrorCode::ENOTDIR);
    }

    // Normalize directory path (ensure it ends with '/' or is "/")
    let dir_prefix = if dir_path == "/" { "/" } else { dir_path };

    let mut entries = Vec::new();

    // Find all files that are direct children of this directory
    for file in FILES.iter() {
        if file.path == dir_path {
            continue; // Skip the directory itself
        }

        // Check if this file is a direct child
        if dir_prefix == "/" {
            // For root directory, find entries with exactly one '/'
            if file.path.starts_with('/') && file.path[1..].find('/').is_none() {
                let name = &file.path[1..]; // Strip leading '/'
                if !name.is_empty() {
                    entries.push((String::from(name), file.file_type));
                }
            }
        } else {
            // For subdirectories, check if path starts with dir_prefix + '/'
            let prefix_with_slash = alloc::format!("{}/", dir_prefix);
            if file.path.starts_with(&prefix_with_slash) {
                let remainder = &file.path[prefix_with_slash.len()..];
                // Check if it's a direct child (no more '/' in remainder)
                if !remainder.contains('/') {
                    entries.push((String::from(remainder), file.file_type));
                }
            }
        }
    }

    // Add dynamic files in /tmp if listing /tmp
    if dir_path == "/tmp" {
        let dynamic = DYNAMIC_FILES.lock();
        if let Some(ref map) = *dynamic {
            for (path, _node_index) in map.iter() {
                if path.starts_with("/tmp/") {
                    let name = &path[5..]; // Strip "/tmp/"
                    if !name.contains('/') {
                        // Direct child of /tmp
                        entries.push((String::from(name), FileType::File));
                    }
                }
            }
        }
    }

    Ok(entries)
}

/// Resolve a relative path against a current working directory
/// Returns an absolute path
pub fn resolve_path(cwd: &str, path: &str) -> Result<alloc::string::String, ErrorCode> {
    // If path is absolute, use it directly
    if path.starts_with('/') {
        return normalize_path(path);
    }

    // Otherwise, prepend cwd
    let combined = if cwd.ends_with('/') {
        alloc::format!("{}{}", cwd, path)
    } else {
        alloc::format!("{}/{}", cwd, path)
    };

    normalize_path(&combined)
}

/// Normalize a path by resolving . and .. components
/// Prevents escaping beyond root (/)
pub fn normalize_path(path: &str) -> Result<alloc::string::String, ErrorCode> {
    use alloc::string::String;
    use alloc::vec::Vec;

    if !path.starts_with('/') {
        return Err(ErrorCode::EINVAL);
    }

    let mut components = Vec::new();

    for component in path.split('/') {
        match component {
            "" | "." => {
                // Skip empty components and current directory
            }
            ".." => {
                // Go up one level (but don't go above root)
                if !components.is_empty() {
                    components.pop();
                }
            }
            name => {
                components.push(name);
            }
        }
    }

    // Build the normalized path
    if components.is_empty() {
        Ok(String::from("/"))
    } else {
        let mut result = String::from("/");
        for (i, component) in components.iter().enumerate() {
            if i > 0 {
                result.push('/');
            }
            result.push_str(component);
        }
        Ok(result)
    }
}

/// Validate that a path exists and is a directory
pub fn validate_directory(path: &str) -> Result<(), ErrorCode> {
    let (_, node) = lookup_node(path).ok_or(ErrorCode::ENOENT)?;
    if node.file_type != FileType::Directory {
        return Err(ErrorCode::ENOTDIR);
    }
    Ok(())
}

/// Unlink (delete) a file or empty directory
pub fn unlink_path(path: &str) -> Result<(), ErrorCode> {
    // Check if path is on a mounted filesystem
    if let Some((_mount, rel_path, fs_type)) = crate::mount::resolve_mount_path(path) {
        match fs_type {
            crate::mount::FsType::Disk => {
                // Disk filesystem is read-only
                return Err(ErrorCode::EROFS);
            }
            crate::mount::FsType::Tmpfs => {
                // Extract parent path and filename
                let (parent, name) = if let Some(pos) = rel_path.rfind('/') {
                    let parent = &rel_path[..pos];
                    let name = &rel_path[pos + 1..];
                    (if parent.is_empty() { "/" } else { parent }, name)
                } else {
                    ("/", rel_path.as_str())
                };

                return crate::mount::tmpfs_unlink(parent, name);
            }
        }
    }

    // For in-memory filesystem, we can only unlink dynamic files in /tmp
    if !path.starts_with("/tmp/") {
        return Err(ErrorCode::EACCES);
    }

    // Check if file exists in dynamic files
    let mut dynamic = DYNAMIC_FILES.lock();
    if let Some(ref mut map) = *dynamic {
        if let Some(node_index) = map.remove(path) {
            // Remove from writable files storage
            let mut files = WRITABLE_FILES.lock();
            if let Some(ref mut store) = *files {
                store.remove(&node_index);
            }
            return Ok(());
        }
    }

    Err(ErrorCode::ENOENT)
}

/// Change file mode (chmod)
pub fn chmod_path(path: &str, new_mode: u16) -> Result<(), ErrorCode> {
    // Check if path is on a mounted filesystem
    if let Some((_mount, rel_path, fs_type)) = crate::mount::resolve_mount_path(path) {
        match fs_type {
            crate::mount::FsType::Disk => {
                // Disk filesystem is read-only
                return Err(ErrorCode::EROFS);
            }
            crate::mount::FsType::Tmpfs => {
                // Change mode in tmpfs
                let inode = crate::mount::tmpfs_lookup(&rel_path)?;
                return crate::mount::tmpfs_chmod(inode, new_mode);
            }
        }
    }

    // For in-memory filesystem, store mode in FILE_MODES
    let (node_index, node) = lookup_node(path).ok_or(ErrorCode::ENOENT)?;

    // Preserve file type bits, only change permission bits
    let file_type_bits = node.mode & S_IFMT;
    let permission_bits = new_mode & 0o777;
    let final_mode = file_type_bits | permission_bits;

    // Store the new mode
    let mut modes = FILE_MODES.lock();
    let mode_map = modes.get_or_insert_with(BTreeMap::new);
    mode_map.insert(node_index, final_mode);

    Ok(())
}

/// Change file ownership (chown)
pub fn chown_path(path: &str, uid: u32, gid: u32) -> Result<(), ErrorCode> {
    // Check if path is on a mounted filesystem
    if let Some((_mount, _rel_path, fs_type)) = crate::mount::resolve_mount_path(path) {
        match fs_type {
            crate::mount::FsType::Disk => {
                // Disk filesystem is read-only
                return Err(ErrorCode::EROFS);
            }
            crate::mount::FsType::Tmpfs => {
                // Tmpfs doesn't support ownership yet - would need to add
                return Err(ErrorCode::ENOSYS);
            }
        }
    }

    // For in-memory filesystem, store ownership in FILE_OWNERSHIP
    let (node_index, node) = lookup_node(path).ok_or(ErrorCode::ENOENT)?;

    // Get current ownership
    let current_ownership = {
        let ownership = FILE_OWNERSHIP.lock();
        if let Some(ref owner_map) = *ownership {
            owner_map.get(&node_index).copied().unwrap_or((node.uid, node.gid))
        } else {
            (node.uid, node.gid)
        }
    };

    // Apply changes (u32::MAX means "don't change")
    let new_uid = if uid == u32::MAX { current_ownership.0 } else { uid };
    let new_gid = if gid == u32::MAX { current_ownership.1 } else { gid };

    // Store the new ownership
    let mut ownership = FILE_OWNERSHIP.lock();
    let owner_map = ownership.get_or_insert_with(BTreeMap::new);
    owner_map.insert(node_index, (new_uid, new_gid));

    Ok(())
}

/// Get file metadata by path
pub fn stat_path(path: &str) -> Result<FileMetadata, ErrorCode> {
    // Check if path is on a mounted filesystem
    if let Some((_mount, rel_path, fs_type)) = crate::mount::resolve_mount_path(path) {
        match fs_type {
            crate::mount::FsType::Disk => {
                let inode = crate::mount::diskfs_lookup(&rel_path)?;
                return crate::mount::diskfs_stat(inode);
            }
            crate::mount::FsType::Tmpfs => {
                let inode = crate::mount::tmpfs_lookup(&rel_path)?;
                return crate::mount::tmpfs_stat(inode);
            }
        }
    }

    let (node_index, node) = lookup_node(path).ok_or(ErrorCode::ENOENT)?;

    // Get mode from FILE_MODES if it exists, otherwise use node mode
    let mode = {
        let modes = FILE_MODES.lock();
        if let Some(ref mode_map) = *modes {
            mode_map.get(&node_index).copied().unwrap_or(node.mode)
        } else {
            node.mode
        }
    };

    // Get ownership from FILE_OWNERSHIP if it exists, otherwise use node ownership
    let (uid, gid) = {
        let ownership = FILE_OWNERSHIP.lock();
        if let Some(ref owner_map) = *ownership {
            owner_map.get(&node_index).copied().unwrap_or((node.uid, node.gid))
        } else {
            (node.uid, node.gid)
        }
    };

    // Check if this is a dynamic file with data in WRITABLE_FILES
    let files = WRITABLE_FILES.lock();
    if let Some(ref store) = *files {
        if let Some(file_data) = store.get(&node_index) {
            return Ok(FileMetadata {
                file_type: FileType::File,
                size: file_data.len() as u64,
                mode,
                uid,
                gid,
            });
        }
    }

    // Fall back to static node data
    Ok(FileMetadata {
        file_type: node.file_type,
        size: if node.file_type == FileType::Directory { 0 } else { node.data.len() as u64 },
        mode,
        uid,
        gid,
    })
}

/// Get file metadata by file descriptor
pub fn fstat_fd(table: &FdTable, fd: i32) -> Result<FileMetadata, ErrorCode> {
    let kind = table.get(fd)?;
    match kind {
        FdKind::File(open, _) => {
            // Get mode from FILE_MODES if it exists, otherwise use node mode
            let node = FILES.get(open.node_index).ok_or(ErrorCode::ENOENT)?;
            let mode = {
                let modes = FILE_MODES.lock();
                if let Some(ref mode_map) = *modes {
                    mode_map.get(&open.node_index).copied().unwrap_or(node.mode)
                } else {
                    node.mode
                }
            };

            // Get ownership from FILE_OWNERSHIP if it exists, otherwise use node ownership
            let (uid, gid) = {
                let ownership = FILE_OWNERSHIP.lock();
                if let Some(ref owner_map) = *ownership {
                    owner_map.get(&open.node_index).copied().unwrap_or((node.uid, node.gid))
                } else {
                    (node.uid, node.gid)
                }
            };

            // Check if this is a dynamic file with data in WRITABLE_FILES
            let files = WRITABLE_FILES.lock();
            if let Some(ref store) = *files {
                if let Some(file_data) = store.get(&open.node_index) {
                    return Ok(FileMetadata {
                        file_type: FileType::File,
                        size: file_data.len() as u64,
                        mode,
                        uid,
                        gid,
                    });
                }
            }

            // Fall back to static node
            Ok(FileMetadata {
                file_type: node.file_type,
                size: if node.file_type == FileType::Directory {
                    0
                } else {
                    node.data.len() as u64
                },
                mode,
                uid,
                gid,
            })
        }
        FdKind::Directory(open) => {
            let node = FILES.get(open.node_index).ok_or(ErrorCode::ENOENT)?;
            let mode = {
                let modes = FILE_MODES.lock();
                if let Some(ref mode_map) = *modes {
                    mode_map.get(&open.node_index).copied().unwrap_or(node.mode)
                } else {
                    node.mode
                }
            };
            // Get ownership from FILE_OWNERSHIP if it exists, otherwise use node ownership
            let (uid, gid) = {
                let ownership = FILE_OWNERSHIP.lock();
                if let Some(ref owner_map) = *ownership {
                    owner_map.get(&open.node_index).copied().unwrap_or((node.uid, node.gid))
                } else {
                    (node.uid, node.gid)
                }
            };
            Ok(FileMetadata { file_type: node.file_type, size: 0, mode, uid, gid })
        }
        FdKind::DiskFile(open) | FdKind::DiskDirectory(open) => {
            crate::mount::diskfs_stat(open.inode)
        }
        FdKind::TmpfsFile(open) | FdKind::TmpfsDirectory(open) => {
            crate::mount::tmpfs_stat(open.inode)
        }
        FdKind::PipeRead(_) | FdKind::PipeWrite(_) => {
            // Pipes don't have traditional stat metadata
            Err(ErrorCode::EBADF)
        }
    }
}

/// Read an entire file into a Vec
///
/// This function reads a complete file from any VFS backend (in-memory, disk, or tmpfs)
/// and returns it as a Vec<u8>. This is useful for loading ELF binaries for exec().
///
/// # Arguments
///
/// * `path` - Absolute path to the file
///
/// # Returns
///
/// The complete file contents as a Vec<u8>, or an error code
pub fn read_file_to_vec(path: &str) -> Result<alloc::vec::Vec<u8>, ErrorCode> {
    use alloc::vec::Vec;

    // First get the file size
    let metadata = stat_path(path)?;

    if metadata.file_type != FileType::File {
        return Err(ErrorCode::EISDIR);
    }

    let file_size = metadata.size as usize;
    if file_size == 0 {
        return Ok(Vec::new());
    }

    // Allocate buffer for the entire file
    let mut buffer = Vec::with_capacity(file_size);
    buffer.resize(file_size, 0);

    // Check if path is on a mounted filesystem
    if let Some((_mount, rel_path, fs_type)) = crate::mount::resolve_mount_path(path) {
        match fs_type {
            crate::mount::FsType::Disk => {
                // Read from disk filesystem
                let inode = crate::mount::diskfs_lookup(&rel_path)?;
                let bytes_read = crate::mount::diskfs_read(inode, 0, &mut buffer)?;
                buffer.truncate(bytes_read);
                return Ok(buffer);
            }
            crate::mount::FsType::Tmpfs => {
                // Read from tmpfs
                let inode = crate::mount::tmpfs_lookup(&rel_path)?;
                let bytes_read = crate::mount::tmpfs_read(inode, 0, &mut buffer)?;
                buffer.truncate(bytes_read);
                return Ok(buffer);
            }
        }
    }

    // Read from in-memory filesystem
    let data = lookup(path).ok_or(ErrorCode::ENOENT)?;
    buffer.clear();
    buffer.extend_from_slice(data);

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fd_allocation_and_reuse() {
        let mut table = FdTable::new();
        let fd1 = table.open_node(0).expect("open should succeed");
        let fd2 = table.open_node(0).expect("open should succeed");
        assert_eq!(fd1, 3);
        assert_eq!(fd2, 4);

        table.close(fd1).expect("close should succeed");
        let fd3 = table.open_node(0).expect("open should reuse");
        assert_eq!(fd3, 3);
    }

    #[test]
    fn test_open_path_lookup() {
        let mut table = FdTable::new();
        let fd = open_path(&mut table, "/etc/motd", 0, 0).expect("motd should exist");
        assert!(fd >= 3);
        let err = open_path(&mut table, "/nope", 0, 0).unwrap_err();
        assert_eq!(err, ErrorCode::ENOENT);
    }

    #[test]
    fn test_read_offsets_and_eof() {
        let mut table = FdTable::new();
        let fd = open_path(&mut table, "/etc/version", 0, 0).expect("version should exist");

        let mut buf = [0u8; 64];
        let n = table.read(fd, &mut buf[..6]).expect("read should succeed");
        assert_eq!(n, 6);
        assert_eq!(&buf[..6], b"PandaO");

        let n = table.read(fd, &mut buf[..64]).expect("read should succeed");
        assert_eq!(&buf[..n], b"S 0.1.0\r\n");

        let n = table.read(fd, &mut buf[..4]).expect("read should succeed");
        assert_eq!(n, 0);
    }

    #[test]
    fn test_write_with_append_flag() {
        let mut table = FdTable::new();

        // Create a writable file with O_CREAT | O_WRONLY
        let fd = open_path_with_flags(&mut table, "/tmp/appendtest", O_CREAT | O_WRONLY, 0, 0)
            .expect("create should succeed");

        // Write some initial data
        let n = table.write(fd, b"Hello").expect("write should succeed");
        assert_eq!(n, 5);

        // Close and reopen with O_APPEND
        table.close(fd).expect("close should succeed");

        let fd = open_path_with_flags(&mut table, "/tmp/appendtest", O_WRONLY | O_APPEND, 0, 0)
            .expect("open with append should succeed");

        // Write more data - should append regardless of offset
        let n = table.write(fd, b" World").expect("write should succeed");
        assert_eq!(n, 6);

        // Close and reopen for reading
        table.close(fd).expect("close should succeed");

        let fd = open_path_with_flags(&mut table, "/tmp/appendtest", O_RDONLY, 0, 0)
            .expect("open for read should succeed");

        // Read and verify the entire content
        let mut buf = [0u8; 64];
        let n = table.read(fd, &mut buf).expect("read should succeed");
        assert_eq!(n, 11);
        assert_eq!(&buf[..n], b"Hello World");

        table.close(fd).expect("close should succeed");
    }

    #[test]
    fn test_append_ignores_seek() {
        let mut table = FdTable::new();

        // Create file with initial content
        let fd = open_path_with_flags(&mut table, "/tmp/seektest", O_CREAT | O_WRONLY, 0, 0)
            .expect("create should succeed");
        table.write(fd, b"ABCD").expect("write should succeed");
        table.close(fd).expect("close should succeed");

        // Open with O_APPEND
        let fd = open_path_with_flags(&mut table, "/tmp/seektest", O_WRONLY | O_APPEND, 0, 0)
            .expect("open with append should succeed");

        // Seek to beginning
        table.set_offset(fd, 0).expect("seek should succeed");

        // Write should still append at end, not at offset 0
        table.write(fd, b"XY").expect("write should succeed");

        table.close(fd).expect("close should succeed");

        // Read and verify - should be "ABCDXY", not "XYCD"
        let fd = open_path_with_flags(&mut table, "/tmp/seektest", O_RDONLY, 0, 0)
            .expect("open for read should succeed");
        let mut buf = [0u8; 64];
        let n = table.read(fd, &mut buf).expect("read should succeed");
        assert_eq!(&buf[..n], b"ABCDXY");

        table.close(fd).expect("close should succeed");
    }

    #[test]
    fn test_error_codes() {
        let mut table = FdTable::new();
        for _ in FIRST_NONSTD_FD..MAX_FDS {
            table.open_node(0).expect("open should succeed");
        }
        let err = table.open_node(0).unwrap_err();
        assert_eq!(err, ErrorCode::EMFILE);

        let mut buf = [0u8; 1];
        let err = table.read(99, &mut buf).unwrap_err();
        assert_eq!(err, ErrorCode::EBADF);

        let err = table.close(1).unwrap_err();
        assert_eq!(err, ErrorCode::EINVAL);
    }
}
