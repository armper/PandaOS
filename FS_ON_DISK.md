# On-Disk Filesystem Format

## Overview

PandaOS uses a simple persistent filesystem for /home with the following properties:
- **Block size**: 512 bytes (matching sector size)
- **Max file size**: ~32KB (64 direct blocks × 512 bytes)
- **Max files**: 256 inodes
- **Total size**: ~512KB (1024 blocks)

## Design Goals

1. **Correctness over performance**: Simple, deterministic behavior
2. **Minimal complexity**: Direct blocks only, no indirect blocks
3. **Persistent**: Survives reboots
4. **Consistent**: Write-through with proper ordering

## On-Disk Layout

```
+------------------+ Sector 0
| Superblock       | 512 bytes
+------------------+ Sector 1
| Inode Bitmap     | 512 bytes (4096 bits = 4096 max inodes)
+------------------+ Sector 2
| Block Bitmap     | 512 bytes (4096 bits = 4096 max blocks)
+------------------+ Sectors 3-34
| Inode Table      | 32 sectors × 8 inodes/sector = 256 inodes
+------------------+ Sector 35+
| Data Blocks      | ~989 sectors available for data
+------------------+
```

### Sector Allocation

- **Sector 0**: Superblock
- **Sector 1**: Inode bitmap (256 bits used)
- **Sector 2**: Block bitmap (1024 bits used)
- **Sectors 3-34**: Inode table (256 inodes × 64 bytes = 16384 bytes = 32 sectors)
- **Sectors 35-1023**: Data blocks (989 blocks)

Total: 1024 sectors = 512KB

## Superblock Structure

**Location**: Sector 0 (512 bytes)

```rust
struct Superblock {
    magic: u32,           // 0x50414E44 ("PAND")
    version: u32,         // Version 2 (v1 was read-only)
    block_size: u32,      // 512 bytes
    total_blocks: u32,    // Total blocks in filesystem (1024)
    inode_count: u32,     // Total inodes (256)
    free_blocks: u32,     // Number of free data blocks
    free_inodes: u32,     // Number of free inodes
    first_data_block: u32,// First data block sector (35)
    inode_bitmap: u32,    // Inode bitmap sector (1)
    block_bitmap: u32,    // Block bitmap sector (2)
    inode_table: u32,     // Inode table start sector (3)
    root_inode: u32,      // Root directory inode (1)
    padding: [u8; 468],   // Pad to 512 bytes
}
```

## Inode Structure

**Location**: Sectors 3-34 (32 sectors, 8 inodes per sector)

**Size**: 64 bytes per inode

```rust
struct Inode {
    inode_num: u32,       // Inode number (1-indexed)
    file_type: u32,       // 1=file, 2=directory
    mode: u16,            // Permission bits (POSIX)
    uid: u16,             // Owner user ID (always 0)
    gid: u16,             // Owner group ID (always 0)
    link_count: u16,      // Hard link count (always 1)
    size: u32,            // File size in bytes
    blocks_used: u32,     // Number of blocks allocated
    direct_blocks: [u32; 8], // Direct block pointers (8 × 4 = 32 bytes)
    padding: [u8; 8],     // Pad to 64 bytes
}
```

### Inode Numbers

- Inode 0: Reserved/invalid
- Inode 1: Root directory ("/")
- Inodes 2-255: Available for files and directories

### File Types

- `1`: Regular file
- `2`: Directory

### Maximum File Size

With 8 direct blocks of 512 bytes each:
- **Max file size** = 8 × 512 = 4096 bytes = 4KB

For larger files, additional blocks can be added up to 64 direct blocks (future extension).

## Inode Bitmap

**Location**: Sector 1 (512 bytes = 4096 bits)

- **Used bits**: First 256 bits (32 bytes)
- **Format**: Bit `i` set = inode `i` is allocated
- **Bit 0**: Always set (inode 0 is reserved)
- **Bit 1**: Set when root directory exists

## Block Bitmap

**Location**: Sector 2 (512 bytes = 4096 bits)

- **Used bits**: First 1024 bits (128 bytes)
- **Format**: Bit `i` set = data block `i` is allocated
- **Block numbering**: Relative to `first_data_block` (sector 35)

## Directory Entry Format

Directories contain a list of entries in their data blocks:

```rust
struct DirEntry {
    inode: u32,           // Inode number (0 = empty/deleted)
    name_len: u8,         // Name length (1-255)
    file_type: u8,        // 1=file, 2=directory
    padding: u16,         // Align to 4 bytes
    name: [u8; 248],      // Name (null-padded)
}
```

**Entry size**: 256 bytes

**Entries per block**: 512 / 256 = 2 entries per block

### Special Entries

- `.` (current directory): Points to self
- `..` (parent directory): Points to parent (or self for root)

## Allocation Strategy

### Inode Allocation

1. Search inode bitmap for first free bit
2. Set bit in bitmap
3. Write updated bitmap to disk
4. Initialize inode entry
5. Write inode to disk
6. Update superblock free_inodes counter

### Block Allocation

1. Search block bitmap for first free bit
2. Set bit in bitmap
3. Write updated bitmap to disk
4. Zero out block data (optional, for security)
5. Update superblock free_blocks counter

### Deallocation

1. Clear bit in bitmap
2. Write updated bitmap to disk
3. Update superblock counters

## Write Ordering

To ensure consistency, writes must be ordered:

1. **Data blocks first**: Write file/directory data
2. **Inode second**: Update inode with new size/blocks
3. **Bitmap third**: Mark blocks/inodes as allocated
4. **Superblock last**: Update free counts

This ensures that on crash:
- No blocks are leaked (bitmap not updated)
- No inodes are leaked (bitmap not updated)
- Files may be incomplete but not corrupt

## Filesystem Operations

### Creating a File

1. Allocate inode
2. Initialize inode (type=file, size=0, mode=0644)
3. Add directory entry to parent directory
4. Update parent directory inode size
5. Sync changes to disk

### Writing to a File

1. Calculate blocks needed
2. Allocate blocks if needed
3. Write data to blocks
4. Update inode size and blocks_used
5. Sync changes to disk

### Deleting a File

1. Read inode to get block list
2. Free all data blocks
3. Free inode
4. Remove directory entry from parent
5. Update parent directory size
6. Sync changes to disk

### Creating a Directory

1. Allocate inode
2. Initialize inode (type=directory, size=512, mode=0755)
3. Allocate one data block
4. Write `.` and `..` entries
5. Add directory entry to parent
6. Sync changes to disk

### Renaming

1. Verify source exists
2. Verify destination doesn't exist (or is empty directory)
3. Add new directory entry
4. Remove old directory entry
5. Update parent directory sizes
6. Sync changes to disk

## Limitations

1. **No indirect blocks**: Max file size is 4KB (8 blocks)
2. **No journaling**: Crash during write may leave inconsistent state
3. **No fsck**: No filesystem check/repair tool
4. **No symlinks**: Only files and directories
5. **No hard links**: link_count always 1
6. **No permissions inheritance**: Must set explicitly
7. **No timestamps**: No atime/mtime/ctime
8. **No extended attributes**: No xattrs
9. **Fixed size**: 512KB total, cannot grow

## Future Extensions

1. **Indirect blocks**: Support larger files
2. **Timestamps**: Add atime/mtime/ctime to inodes
3. **Journaling**: Add write-ahead log
4. **fsck**: Add filesystem checker
5. **Larger filesystem**: Support > 512KB
6. **Variable block size**: Support 4KB blocks

## Error Handling

### ENOSPC (No space left on device)

Returned when:
- No free inodes available
- No free blocks available
- File would exceed max size

### EIO (I/O error)

Returned when:
- Block device read/write fails
- Corrupted metadata detected

### EINVAL (Invalid argument)

Returned when:
- Invalid inode number
- Invalid block number
- Corrupted data structures

## Testing Strategy

1. **Unit tests**: Test bitmap, inode, block allocation
2. **Integration tests**: Test file operations
3. **Persistence tests**: Verify data survives reboot
4. **Stress tests**: Fill filesystem to capacity
5. **Error injection**: Test error handling

## References

- ext2 filesystem: https://www.nongnu.org/ext2-doc/ext2.html
- MINIX filesystem: https://en.wikipedia.org/wiki/MINIX_file_system
