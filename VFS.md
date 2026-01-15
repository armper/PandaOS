# VFS

## Overview

PandaOS provides a tiny read-only virtual filesystem backed by an in-memory table of static
file nodes. It supports absolute-path lookup, per-process file descriptors, sequential
reads with per-fd offsets, and directory listing via `getdents64`.

## Invariants

- Paths are absolute and matched by exact string equality.
- The filesystem is read-only; no write, create, or delete operations exist.
- File data is embedded as static byte slices.
- Directories are represented as FileNodes with type Directory.
- Each process owns a fixed-size FD table (16 entries).
- FDs 0/1/2 are reserved for stdin/stdout/stderr and are not stored in the table.
- open() returns the lowest available FD >= 3.
- close(0/1/2) returns EINVAL.
- read() advances the per-fd offset and returns 0 on EOF.
- read() on directories returns EISDIR (use getdents64 instead).

## Directory Structure

The VFS contains three directories:
- `/` - Root directory
- `/bin` - Binary executables directory
- `/etc` - Configuration files directory

Directory listing is performed via the `getdents64` syscall (Linux-compatible).

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
