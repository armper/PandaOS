//! Minimal in-memory filesystem for exec() and VFS reads.

use crate::pipe::PipeId;
use crate::syscall::ErrorCode;

pub const MAX_FDS: usize = 16;
pub const FIRST_NONSTD_FD: usize = 3;

pub struct FileNode {
    pub path: &'static str,
    pub data: &'static [u8],
}

#[derive(Clone, Copy, Debug)]
pub struct OpenFile {
    pub node_index: usize,
    pub offset: usize,
}

/// File descriptor kinds
#[derive(Clone, Copy, Debug)]
pub enum FdKind {
    /// Regular file
    File(OpenFile),
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

    pub fn open_node(&mut self, node_index: usize) -> Result<i32, ErrorCode> {
        let fd = self.allocate_fd()?;
        self.entries[fd] = Some(FdKind::File(OpenFile { node_index, offset: 0 }));
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
                FdKind::File(_) => {
                    // Files don't need cleanup
                }
            }
            Ok(())
        } else {
            Err(ErrorCode::EBADF)
        }
    }

    pub fn read(&mut self, fd: i32, count: usize) -> Result<&'static [u8], ErrorCode> {
        if fd < 0 || fd as usize >= MAX_FDS {
            return Err(ErrorCode::EBADF);
        }
        if fd < FIRST_NONSTD_FD as i32 {
            return Err(ErrorCode::EBADF);
        }
        let kind = self.entries[fd as usize].ok_or(ErrorCode::EBADF)?;
        match kind {
            FdKind::File(open) => {
                let node = FILES.get(open.node_index).ok_or(ErrorCode::ENOENT)?;
                if open.offset >= node.data.len() || count == 0 {
                    return Ok(&[]);
                }
                let available = node.data.len() - open.offset;
                let to_read = available.min(count);
                let start = open.offset;
                let end = start + to_read;
                self.entries[fd as usize] =
                    Some(FdKind::File(OpenFile { node_index: open.node_index, offset: end }));
                Ok(&node.data[start..end])
            }
            FdKind::PipeRead(_) | FdKind::PipeWrite(_) => {
                // Pipes are handled through separate syscall path
                Err(ErrorCode::EBADF)
            }
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
            FdKind::File(open) => {
                self.entries[newfd as usize] = Some(FdKind::File(open));
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
        let mut new_table = *self;

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
                    FdKind::File(_) => {
                        // Files don't need refcounting
                    }
                }
            }
        }

        Ok(new_table)
    }
}

static FILES: &[FileNode] = &[
    FileNode { path: "/init", data: include_bytes!("../../userland/bin/init") },
    FileNode { path: "/bin/sh", data: include_bytes!("../../userland/bin/sh") },
    FileNode { path: "/bin/cat", data: include_bytes!("../../userland/bin/cat") },
    FileNode { path: "/bin/true", data: include_bytes!("../../userland/bin/true") },
    FileNode { path: "/bin/echo", data: include_bytes!("../../userland/bin/echo") },
    FileNode { path: "/bin/wc", data: include_bytes!("../../userland/bin/wc") },
    FileNode { path: "/etc/motd", data: b"Welcome to PandaOS.\r\nType 'help' for commands.\r\n" },
    FileNode { path: "/etc/version", data: b"PandaOS 0.1.0\r\n" },
];

/// Look up a file by absolute path.
pub fn lookup(path: &str) -> Option<&'static [u8]> {
    FILES.iter().find(|entry| entry.path == path).map(|entry| entry.data)
}

/// Look up a file node by absolute path.
pub fn lookup_node(path: &str) -> Option<(usize, &'static FileNode)> {
    FILES.iter().enumerate().find(|(_, entry)| entry.path == path)
}

/// Open a file by path into a file descriptor table.
pub fn open_path(table: &mut FdTable, path: &str) -> Result<i32, ErrorCode> {
    let (node_index, _node) = lookup_node(path).ok_or(ErrorCode::ENOENT)?;
    table.open_node(node_index)
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
        let fd = open_path(&mut table, "/etc/motd").expect("motd should exist");
        assert!(fd >= 3);
        let err = open_path(&mut table, "/nope").unwrap_err();
        assert_eq!(err, ErrorCode::ENOENT);
    }

    #[test]
    fn test_read_offsets_and_eof() {
        let mut table = FdTable::new();
        let fd = open_path(&mut table, "/etc/version").expect("version should exist");
        let first = table.read(fd, 6).expect("read should succeed");
        assert_eq!(first, b"PandaO");
        let second = table.read(fd, 64).expect("read should succeed");
        assert_eq!(second, b"S 0.1.0\r\n");
        let eof = table.read(fd, 4).expect("read should succeed");
        assert_eq!(eof.len(), 0);
    }

    #[test]
    fn test_error_codes() {
        let mut table = FdTable::new();
        for _ in FIRST_NONSTD_FD..MAX_FDS {
            table.open_node(0).expect("open should succeed");
        }
        let err = table.open_node(0).unwrap_err();
        assert_eq!(err, ErrorCode::EMFILE);

        let err = table.read(99, 1).unwrap_err();
        assert_eq!(err, ErrorCode::EBADF);

        let err = table.close(1).unwrap_err();
        assert_eq!(err, ErrorCode::EINVAL);
    }
}
