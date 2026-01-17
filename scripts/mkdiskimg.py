#!/usr/bin/env python3
"""
Generate a simple disk image for PandaOS diskfs testing.

This script creates a disk image with the custom filesystem format:
- Superblock at sector 0
- Inode table starting at sector 1
- Data blocks starting after inodes

Layout:
- Sector 0: Superblock
- Sectors 1-N: Inode table (8 inodes per sector)
- Sectors N+1...: Data blocks
"""

import struct
import sys
from pathlib import Path

# Constants matching diskfs.rs
FS_MAGIC = 0x50414E44  # "PAND"
FS_VERSION = 1
SECTOR_SIZE = 512
MAX_DIRECT_BLOCKS = 28  # Must match diskfs.rs
INODE_SIZE = 128
INODES_PER_SECTOR = SECTOR_SIZE // INODE_SIZE  # 4

# Inode types
INODE_FILE = 1
INODE_DIR = 2


class Inode:
    """Represents an on-disk inode."""
    
    def __init__(self, inode_num, file_type, size, direct_blocks):
        self.inode_num = inode_num
        self.file_type = file_type
        self.size = size
        self.direct_blocks = direct_blocks[:MAX_DIRECT_BLOCKS]
        # Pad to 10 blocks
        while len(self.direct_blocks) < MAX_DIRECT_BLOCKS:
            self.direct_blocks.append(0)
    
    def to_bytes(self):
        """Serialize inode to 128 bytes."""
        data = struct.pack('<I', self.inode_num)  # 4 bytes
        data += struct.pack('<I', self.file_type)  # 4 bytes
        data += struct.pack('<Q', self.size)       # 8 bytes
        for block in self.direct_blocks:           # 112 bytes (28 * 4)
            data += struct.pack('<I', block)
        # No extra padding needed for 128-byte inode
        assert len(data) == INODE_SIZE
        return data


class DirEntry:
    """Represents a directory entry."""
    
    def __init__(self, inode_num, name):
        self.inode_num = inode_num
        self.name = name.encode('utf-8')
    
    def to_bytes(self):
        """Serialize directory entry."""
        data = struct.pack('<I', self.inode_num)
        data += struct.pack('<B', len(self.name))
        data += self.name
        # Align to 4-byte boundary
        padding = (4 - (len(data) % 4)) % 4
        data += b'\x00' * padding
        return data


class DiskImage:
    """Builds a disk image with the custom filesystem."""
    
    def __init__(self, size_sectors=1024):
        self.sectors = [bytearray(SECTOR_SIZE) for _ in range(size_sectors)]
        self.inodes = []
        self.next_inode = 0
        self.next_data_block = None
        self.root_inode = None
    
    def allocate_inode(self):
        """Allocate a new inode number."""
        inode_num = self.next_inode
        self.next_inode += 1
        return inode_num
    
    def allocate_data_blocks(self, size):
        """Allocate data blocks for given size."""
        if self.next_data_block is None:
            raise RuntimeError("Filesystem not initialized")
        
        blocks_needed = (size + SECTOR_SIZE - 1) // SECTOR_SIZE
        blocks = []
        for _ in range(blocks_needed):
            blocks.append(self.next_data_block)
            self.next_data_block += 1
        return blocks
    
    def write_data_blocks(self, blocks, data):
        """Write data to allocated blocks."""
        offset = 0
        for block_num in blocks:
            chunk = data[offset:offset + SECTOR_SIZE]
            self.sectors[block_num][:len(chunk)] = chunk
            offset += SECTOR_SIZE
    
    def add_file(self, name, content):
        """Add a file and return its inode number."""
        inode_num = self.allocate_inode()
        data = content.encode('utf-8') if isinstance(content, str) else content
        blocks = self.allocate_data_blocks(len(data))
        self.write_data_blocks(blocks, data)
        
        inode = Inode(inode_num, INODE_FILE, len(data), blocks)
        self.inodes.append(inode)
        return inode_num
    
    def add_directory(self, entries):
        """
        Add a directory with given entries.
        entries: list of (inode_num, name) tuples
        Returns the directory inode number.
        """
        inode_num = self.allocate_inode()
        
        # Serialize directory entries
        dir_data = bytearray()
        for entry_inode, entry_name in entries:
            dir_entry = DirEntry(entry_inode, entry_name)
            dir_data.extend(dir_entry.to_bytes())
        
        blocks = self.allocate_data_blocks(len(dir_data))
        self.write_data_blocks(blocks, dir_data)
        
        inode = Inode(inode_num, INODE_DIR, len(dir_data), blocks)
        self.inodes.append(inode)
        return inode_num
    
    def write_inodes(self):
        """Write all inodes to disk."""
        for i, inode in enumerate(self.inodes):
            sector_num = 1 + (i // INODES_PER_SECTOR)
            sector_offset = (i % INODES_PER_SECTOR) * INODE_SIZE
            inode_bytes = inode.to_bytes()
            self.sectors[sector_num][sector_offset:sector_offset + INODE_SIZE] = inode_bytes
    
    def write_superblock(self):
        """Write superblock to sector 0."""
        superblock = struct.pack('<I', FS_MAGIC)
        superblock += struct.pack('<I', FS_VERSION)
        superblock += struct.pack('<I', self.root_inode)
        superblock += struct.pack('<I', len(self.inodes))
        superblock += struct.pack('<I', self.next_data_block)
        # Pad to 512 bytes
        superblock += b'\x00' * (SECTOR_SIZE - len(superblock))
        assert len(superblock) == SECTOR_SIZE
        self.sectors[0] = bytearray(superblock)
    
    def initialize(self):
        """Initialize filesystem with calculated first data block."""
        # Calculate first data block
        inode_sectors = (self.next_inode + INODES_PER_SECTOR - 1) // INODES_PER_SECTOR
        self.next_data_block = 1 + inode_sectors
    
    def finalize(self):
        """Finalize and write all structures."""
        self.write_inodes()
        self.write_superblock()
    
    def save(self, filename):
        """Save disk image to file."""
        with open(filename, 'wb') as f:
            for sector in self.sectors:
                f.write(bytes(sector))


def main():
    """Generate test disk image."""
    print("Generating disk image...")
    
    disk = DiskImage(size_sectors=4096)  # Increased size for binaries

    # Collect userland binaries first so we can compute inode table size up front.
    userland_bin = Path(__file__).parent.parent / "userland" / "bin"
    bin_files = []
    for name in ["init", "sh", "ls", "cat", "echo", "wc", "true", "spin", "pingpong", "preempt_test"]:
        p = userland_bin / name
        if p.exists():
            bin_files.append((name, p))

    # We will create:
    # - 4 root files (hello.txt, README, test.txt, version)
    # - N /bin files
    # - 2 directories (/bin and /)
    total_inodes = 4 + len(bin_files) + 2
    inode_sectors = (total_inodes + INODES_PER_SECTOR - 1) // INODES_PER_SECTOR
    first_data_block = 1 + inode_sectors
    disk.next_data_block = first_data_block

    # Create files first (to get inode numbers)
    print("  Creating files...")
    hello_inode = disk.add_file("hello.txt", "Hello from disk!\n")
    readme_inode = disk.add_file("README", "This is a test filesystem.\nMounted at /mnt\n")
    test_inode = disk.add_file("test.txt", "Test file content\n")
    version_inode = disk.add_file("version", "PandaOS 0.1.0\n")

    # Create bin directory with actual ELF binaries
    print("  Creating /bin directory with ELF binaries...")
    bin_entries = []
    for name, path in bin_files:
        print(f"    Adding {name} ({path.stat().st_size} bytes)")
        with open(path, 'rb') as f:
            data = f.read()
        inode = disk.add_file(name, data)
        bin_entries.append((inode, name))

    bin_inode = disk.add_directory(bin_entries)

    # Create root directory
    print("  Creating root directory...")
    root_inode = disk.add_directory([
        (hello_inode, "hello.txt"),
        (readme_inode, "README"),
        (test_inode, "test.txt"),
        (version_inode, "version"),
        (bin_inode, "bin"),
    ])

    disk.root_inode = root_inode
    
    # Finalize and write
    disk.finalize()
    
    # Save to file
    output_file = Path(__file__).parent.parent / "fs.img"
    disk.save(output_file)
    
    print(f"\nDisk image created: {output_file}")
    print(f"  Root inode: {root_inode}")
    print(f"  Total inodes: {len(disk.inodes)}")
    print(f"  First data block: {first_data_block}")
    print(f"  Image size: {len(disk.sectors)} sectors ({len(disk.sectors) * SECTOR_SIZE} bytes)")
    
    # Print file listing
    print("\nFiles in image:")
    print(f"  /hello.txt (inode {hello_inode})")
    print(f"  /README (inode {readme_inode})")
    print(f"  /test.txt (inode {test_inode})")
    print(f"  /version (inode {version_inode})")
    print(f"  /bin/ (inode {bin_inode})")
    for inode_num, name in bin_entries:
        print(f"    /bin/{name} (inode {inode_num})")


if __name__ == '__main__':
    main()
