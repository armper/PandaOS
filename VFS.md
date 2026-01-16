# VFS

## Overview

PandaOS provides a virtual filesystem with support for multiple backends:
1. **In-memory filesystem** - Static files compiled into the kernel
2. **Disk filesystem** - Read-only files from block devices

The VFS supports absolute and relative path lookup, per-process file descriptors, 
per-process current working directory, sequential reads with per-fd offsets, directory 
listing via `getdents64`, and file metadata queries via `stat`/`fstat`.

## Filesystem Backends

### In-Memory Filesystem
- Static file nodes compiled into the kernel binary
- Writable /tmp directory for dynamic file creation
- Always mounted at root (/)
- Files embedded as static byte slices

### Disk Filesystem
- Read-only custom filesystem format
- Backed by ATA/IDE block device
- Mounted at /mnt by default
- Custom on-disk layout: superblock, inode table, data blocks

### Mount Point System

The VFS uses a global mount table to track mounted filesystems:
- Path resolution checks mount points first
- Mount boundaries are traversed transparently
- `/mnt` prefix routes to disk filesystem
- All other paths use in-memory filesystem

## Invariants

- Paths are matched by exact string equality after normalization.
- The in-memory filesystem is mostly read-only (except /tmp).
- The disk filesystem is completely read-only.
- Directories are represented as FileNodes with type Directory.
- Each process owns a fixed-size FD table (16 entries).
- Each process has a current working directory (cwd), initialized to `/`.
- FDs 0/1/2 are reserved for stdin/stdout/stderr and are not stored in the table.
- open() returns the lowest available FD >= 3.
- close(0/1/2) returns EINVAL.
- read() advances the per-fd offset and returns 0 on EOF.
- read() on directories returns EISDIR (use getdents64 instead).
- stat()/fstat() return FileMetadata with file type and size.
- Write operations on disk files return EROFS (read-only filesystem).

## File Metadata

The VFS supports minimal file metadata queries:

### FileMetadata Structure
```rust
pub struct FileMetadata {
    pub file_type: FileType,  // File or Directory
    pub size: u64,             // Size in bytes (0 for directories)
}
```

### Syscalls
- `stat(path, buf)` - Get metadata for a file by path (resolved relative to cwd)
- `fstat(fd, buf)` - Get metadata for an open file descriptor

### Metadata Format
Metadata is returned as a 16-byte structure:
- Byte 0: `file_type` (0 = File, 1 = Directory)
- Bytes 1-7: Padding
- Bytes 8-15: `size` (little-endian u64, 0 for directories)

### Usage Example
```asm
; Call stat on a path
mov rax, 4              ; SYS_STAT
lea rdi, [path]         ; path pointer
lea rsi, [stat_buf]     ; buffer for result
syscall

; Check if it's a directory
mov al, byte [stat_buf]
cmp al, 1
je is_directory
```

## Directory Structure

The VFS contains three directories:
- `/` - Root directory (contains: bin, etc)
- `/bin` - Binary executables directory (contains: sh, cat, true, echo, wc, ls)
- `/etc` - Configuration files directory (contains: motd, version)

Directory listing is performed via the `getdents64` syscall (Linux-compatible).

## Working Directory Support

Each process maintains a current working directory (cwd):
- New processes start with cwd = `/`
- `fork()` - child inherits parent's cwd
- `exec()` - preserves cwd
- `chdir(path)` - changes cwd (validates directory exists)
- `getcwd(buf, size)` - returns current cwd

## Path Resolution

Paths are resolved relative to the process's cwd:

**Absolute Paths:**
- Start with `/`
- Used directly after normalization

**Relative Paths:**
- Do not start with `/`
- Prepended with cwd before resolution

**Special Components:**
- `.` - current directory (no-op)
- `..` - parent directory (go up one level)
- Cannot escape root `/`

**Examples:**
```
cwd = "/"
  "bin"     -> "/bin"
  "."       -> "/"
  ".."      -> "/" (can't escape root)
  "/etc"    -> "/etc" (absolute)

cwd = "/bin"
  "."       -> "/bin"
  ".."      -> "/"
  "cat"     -> "/bin/cat"
  "/etc"    -> "/etc" (absolute)
```

## FD Semantics

- `fd 0`: stdin (serial input)
- `fd 1`: stdout (serial output)
- `fd 2`: stderr (serial output)
- `fd >= 3`: read-only files, directories, or pipes

### FD Kinds

The FD table supports four kinds of file descriptors:

1. **File**: Read-only files backed by the in-memory table
   - Tracks per-fd offset for sequential reads
   - Returns EOF (empty slice) when reaching end of file

2. **Directory**: Directory nodes opened for listing
   - Tracks per-fd offset for sequential getdents64 calls
   - Returns entries via getdents64 syscall
   - read() returns EISDIR error

3. **PipeRead**: Read end of a pipe
   - Reads from pipe ring buffer
   - Returns EOF when all writers closed and buffer empty
   - Returns EAGAIN if buffer empty but writers exist

4. **PipeWrite**: Write end of a pipe
   - Writes to pipe ring buffer
   - Returns EPIPE error when all readers closed
   - Returns EAGAIN if buffer full

## Directory Operations

### Opening Directories

Directories can be opened like regular files:
```c
int fd = open("/", O_RDONLY, 0);
```

### Reading Directory Entries

Use `getdents64` syscall to read directory entries:
```c
struct linux_dirent64 {
    uint64_t d_ino;      // Inode number
    uint64_t d_off;      // Offset to next entry
    uint16_t d_reclen;   // Length of this entry
    uint8_t  d_type;     // File type (DT_REG=8, DT_DIR=4)
    char     d_name[];   // Null-terminated filename
};

char buf[1024];
int nread = syscall(SYS_GETDENTS64, fd, buf, sizeof(buf));
```

The kernel returns entries in Linux-compatible format with proper alignment.

### Listing Directory Contents

Example `/bin/ls` implementation:
1. Open "/" directory
2. Call getdents64 to read entries
3. Parse and print each entry name
4. Repeat until getdents64 returns 0 (EOF)

## Fork Behavior

On fork(), the child's FD table is a copy of the parent's with proper refcounting:

- **Files**: Duplicated with independent offsets (standard Unix behavior deviation - we copy instead of sharing)
- **Directories**: Duplicated with independent offsets
- **Pipes**: Reference counts incremented for both read and write ends
  - Parent and child share the same underlying pipe buffer
  - Both can read/write to their respective ends
  - Pipe persists until all ends are closed

This differs from traditional Unix where file descriptors point to a shared
open file table entry. PandaOS uses a simpler per-process offset model for files
but proper reference counting for pipes.

## Pipe Operations

### Creating a Pipe

```c
int pipefd[2];
pipe(pipefd);  // pipefd[0] = read end, pipefd[1] = write end
```

### Using Pipes

- **Write**: `write(pipefd[1], data, len)` - writes to pipe buffer
- **Read**: `read(pipefd[0], buf, len)` - reads from pipe buffer
- **Close**: `close(pipefd[0])` or `close(pipefd[1])` - decrements refcounts

### Duplicating FDs

```c
dup2(oldfd, newfd);  // Copy oldfd to newfd (increments refcounts for pipes)
```

Common use case: Redirect stdin/stdout to pipe ends in child process:
```c
dup2(pipefd[0], 0);  // stdin = pipe read end
dup2(pipefd[1], 1);  // stdout = pipe write end
```

## Fork Behavior (Legacy)

On fork(), the child's FD table is a copy of the parent's. Open file descriptors
are duplicated with their current offsets. Since the FD table stores offsets
per-process, parent and child have independent offsets (they don't share file
position). This differs from traditional Unix where descriptors point to a shared
open file table entry.

## Exec Argument Convention

`execve(path, arg_ptr, _)` accepts a single optional argument string. The kernel copies that
string into user memory at a fixed address before transferring control to the new image:

- `EXEC_ARG_ADDR = 0x7FFF_FFFF_C000`
- The string is NUL-terminated.

User programs (e.g., `/bin/cat`) read the argument from that fixed address.

## Disk Filesystem Format

The disk filesystem uses a simple custom format optimized for read-only access:

### On-Disk Layout

**Sector 0: Superblock (512 bytes)**
- Magic number: `0x50414E44` ("PAND")
- Version: 1
- Root inode number
- Total inode count
- First data block number

**Sectors 1-N: Inode Table**
- 8 inodes per sector (64 bytes each)
- Inode structure:
  - Inode number (4 bytes)
  - File type (4 bytes): 1=file, 2=directory
  - Size in bytes (8 bytes)
  - Direct block pointers (40 bytes): up to 10 blocks
  - Padding (8 bytes)

**Sectors N+1...: Data Blocks**
- File data blocks: raw file contents
- Directory data blocks: array of directory entries
  - Entry structure:
    - Inode number (4 bytes)
    - Name length (1 byte)
    - Name (variable, null-terminated)
    - Padding to 4-byte alignment

### Block Device Interface

The disk filesystem accesses storage through a `BlockDevice` trait:
- Sector size: 512 bytes
- Read-only operations
- ATA/IDE driver implementation for QEMU

### Creating Disk Images

Use `scripts/mkdiskimg.py` to generate disk images:
```bash
python3 scripts/mkdiskimg.py
```

This creates `fs.img` with test files:
- `/hello.txt` - Test file with sample content
- `/README` - Filesystem documentation
- `/test.txt` - Additional test file
- `/bin/` - Directory with placeholder binaries

### QEMU Configuration

The disk image is attached to QEMU via the kernel's `Cargo.toml`:
```toml
run-args = ["-drive", "file=fs.img,format=raw,if=ide"]
```

The ATA driver reads from the primary master disk (IDE port 0x1F0).
