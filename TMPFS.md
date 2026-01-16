# Tmpfs - Temporary Filesystem

## Overview

Tmpfs is a writable in-memory temporary filesystem mounted at `/tmp` in PandaOS. It provides POSIX-like file operations for runtime file creation and manipulation. All data is stored in RAM and is lost on reboot.

## Architecture

### Storage Structure

```
TmpFs
├── nodes: BTreeMap<Inode, TmpFsNode>
│   ├── File { data: Vec<u8> }
│   └── Directory { entries: BTreeMap<String, Inode> }
├── modes: BTreeMap<Inode, u16>
└── root_inode: Inode (always 1)
```

### Key Components

- **Inode**: 32-bit unsigned integer, unique identifier for files and directories
- **TmpFsNode**: Enum representing either a File or Directory
- **Files**: Store data as Vec<u8> with dynamic resizing
- **Directories**: Map child names to their inode numbers

## Mount Table Integration

The global mount table resolves paths to the appropriate filesystem:

```rust
/tmp/file.txt  -> tmpfs backend
/mnt/file.txt  -> disk filesystem backend  
/etc/file.txt  -> in-memory VFS backend
```

Path resolution uses prefix matching:
- Paths starting with `/tmp` route to tmpfs
- Paths starting with `/mnt` route to disk filesystem
- All other paths use in-memory VFS

## Supported Operations

### File Operations

| Operation | Syscall | Description |
|-----------|---------|-------------|
| Create | `open(O_CREAT)` | Create new file with mode |
| Write | `write()` | Write data at offset, auto-extend |
| Read | `read()` | Read data from offset |
| Truncate | `open(O_TRUNC)` | Clear file contents |
| Delete | `unlink()` | Remove file (EISDIR if directory) |
| Seek | `lseek()` | Change file offset |

### Directory Operations

| Operation | Syscall | Description |
|-----------|---------|-------------|
| Create | `mkdir()` | Create directory with mode |
| List | `getdents64()` | Read directory entries |
| Remove | `rmdir()` | Remove empty directory (ENOTEMPTY if not empty) |
| Traverse | Open/stat | Access subdirectories |

### File Descriptor Flags

| Flag | Value | Behavior |
|------|-------|----------|
| O_RDONLY | 0x0000 | Read-only access |
| O_WRONLY | 0x0001 | Write-only access |
| O_RDWR | 0x0002 | Read-write access |
| O_CREAT | 0x0040 | Create if doesn't exist |
| O_TRUNC | 0x0200 | Truncate to 0 on open |
| O_APPEND | 0x0400 | Always write at end |

## Error Handling

### Common Error Codes

| Error | Code | When |
|-------|------|------|
| ENOENT | 2 | Path doesn't exist |
| EEXIST | 17 | File already exists (O_CREAT) |
| EISDIR | 21 | Operation on directory invalid for files |
| ENOTDIR | 20 | Path component not a directory |
| ENOTEMPTY | 39 | Directory not empty (rmdir) |
| EINVAL | 22 | Invalid argument (e.g., name with '/') |
| EXDEV | 18 | Cross-device operation (rename) |

### Error Mapping

```rust
tmpfs error -> ErrorCode -> -errno (syscall return)
```

## Limitations

### Not Implemented

- **Hard links**: No support for multiple names per inode
- **Symbolic links**: No support for symlinks
- **Permissions inheritance**: Files created with explicit mode only
- **Timestamps**: No atime/mtime/ctime tracking
- **Extended attributes**: No xattrs
- **File locking**: No fcntl/flock support
- **Sparse files**: All space allocated immediately
- **Ownership changes**: chown/chgrp not implemented for tmpfs

### Behavioral Constraints

- **Cross-device rename**: Returns EXDEV if source and dest are on different filesystems
- **Directory removal**: Only empty directories can be removed
- **Path length**: No explicit limit, but bounded by available memory
- **File size**: Limited by available RAM
- **Inode exhaustion**: No limit on inode allocation (bounded by RAM)

## Invariants

1. **Root always exists**: Root inode (1) is always a directory
2. **Inode uniqueness**: Each inode number is used only once
3. **Parent validity**: All entries in a directory refer to valid inodes
4. **Name validity**: Names cannot contain '/' or be empty
5. **Directory consistency**: Removing a directory requires it to be empty
6. **Mode preservation**: File type bits (S_IFDIR/S_IFREG) are always set correctly

## Implementation Details

### Inode Allocation

```rust
fn allocate_inode(&mut self) -> Inode {
    let inode = self.next_inode;
    self.next_inode += 1;
    inode
}
```

- Simple monotonic counter
- No reuse of freed inodes
- Safe from overflow in practice (u32 max = 4 billion)

### Path Lookup

```rust
pub fn lookup(&self, base_inode: Inode, path: &str) -> Result<Inode, ErrorCode>
```

- Splits path by '/' and traverses directories
- Returns ENOTDIR if non-directory in path
- Returns ENOENT if any component missing

### Write Behavior

```rust
pub fn write(&mut self, inode: Inode, offset: usize, data: &[u8]) -> Result<usize, ErrorCode>
```

- Extends file if `offset > file.len()`
- Zero-fills gaps between old end and new write position
- O_APPEND is enforced at FD layer (always writes at current size)

## Usage Examples

### Creating and Writing a File

```asm
; open("/tmp/test.txt", O_WRONLY | O_CREAT, 0644)
mov rax, 2                    ; SYS_OPEN
lea rdi, [rel filename]
mov rsi, 0x0041               ; O_WRONLY | O_CREAT
mov rdx, 0o644
syscall
; Returns fd in rax

; write(fd, "hello", 5)
mov rdi, rax                  ; fd
lea rsi, [rel data]
mov rdx, 5
mov rax, 1                    ; SYS_WRITE
syscall
```

### Append Mode

```asm
; open("/tmp/log.txt", O_WRONLY | O_CREAT | O_APPEND, 0644)
mov rax, 2
lea rdi, [rel filename]
mov rsi, 0x0441               ; O_WRONLY | O_CREAT | O_APPEND
mov rdx, 0o644
syscall
```

### Directory Operations

```asm
; mkdir("/tmp/mydir", 0755)
mov rax, 83                   ; SYS_MKDIR
lea rdi, [rel dirname]
mov rsi, 0o755
syscall

; rmdir("/tmp/mydir")
mov rax, 84                   ; SYS_RMDIR
lea rdi, [rel dirname]
syscall
```

## Testing

### Unit Tests

Located in `kernel/src/tmpfs.rs`:
- `test_tmpfs_create_file`
- `test_tmpfs_write_and_read`
- `test_tmpfs_unlink`
- `test_tmpfs_directory`
- `test_tmpfs_truncate`

### Integration Test

`tmpfs_redir_smoke` feature tests:
- File creation via shell redirection
- Append mode (`>>`)
- Directory operations
- File removal
- Piped output to files

Run with:
```bash
TMPFS_REDIR_SMOKE=1 ./scripts/qemu-test.sh
```

## Future Enhancements

Potential improvements (not currently planned):

1. **Permissions**: Full chmod/chown support with ownership tracking
2. **Timestamps**: Add atime/mtime/ctime fields
3. **Quotas**: Limit per-process or total tmpfs usage
4. **Symlinks**: Add symbolic link support
5. **Memory pressure**: Evict tmpfs data under low memory conditions
6. **Persistence**: Optional backing to swap/disk
7. **Hard links**: Multiple directory entries per inode

## References

- VFS implementation: `kernel/src/fs.rs`
- Mount table: `kernel/src/mount.rs`
- Tmpfs core: `kernel/src/tmpfs.rs`
- Syscalls: `kernel/src/syscall.rs`
- Main integration: `kernel/src/main.rs`
