# HomeFS Integration Guide

## Status

**Completed:**
- ✅ Block device write support (ATA + trait)
- ✅ Multi-drive ATA support (master/slave)
- ✅ Complete HomeFS implementation (homefs.rs)
- ✅ Filesystem format documentation (FS_ON_DISK.md)
- ✅ Disk image creation tool (mkhomefs.py)

**Remaining:** VFS integration, QEMU configuration, testing

## Integration Steps

### 1. Mount System Updates (kernel/src/mount.rs)

Add HomeFS to the mount table:

```rust
use crate::homefs::{HomeFs, HomeFsError};

pub enum FsType {
    Disk,      // Existing read-only disk
    Tmpfs,     // Existing writable tmpfs
    Home,      // New persistent writable HomeFS
}

pub struct MountTable {
    mounts: Vec<(String, MountEntry)>,
    disk_fs: Option<DiskFs<AtaDisk>>,
    home_fs: Option<HomeFs<AtaDisk>>,  // ADD THIS
}

pub fn mount_home_at_home() -> Result<(), ErrorCode> {
    // Initialize slave ATA disk for /home
    let disk = unsafe { AtaDisk::new_slave() };
    
    // Try to open existing filesystem, or create new one
    let home_fs = HomeFs::open(disk)
        .or_else(|_| {
            let disk = unsafe { AtaDisk::new_slave() };
            HomeFs::create(disk)
        })
        .map_err(|_| ErrorCode::EIO)?;
    
    let mut table = MOUNT_TABLE.lock();
    let mount_table = table.as_mut().ok_or(ErrorCode::EIO)?;
    mount_table.home_fs = Some(home_fs);
    mount_table.mounts.push((
        String::from("/home"),
        MountEntry { mount_point: "/home", fs_type: FsType::Home }
    ));
    
    Ok(())
}

// Add helper functions for HomeFS operations
pub fn homefs_lookup(path: &str) -> Result<u32, ErrorCode> { ... }
pub fn homefs_read(inode: u32, ...) -> Result<usize, ErrorCode> { ... }
pub fn homefs_write(inode: u32, ...) -> Result<usize, ErrorCode> { ... }
pub fn homefs_create_file(...) -> Result<u32, ErrorCode> { ... }
pub fn homefs_create_directory(...) -> Result<u32, ErrorCode> { ... }
pub fn homefs_unlink(...) -> Result<(), ErrorCode> { ... }
pub fn homefs_rename(...) -> Result<(), ErrorCode> { ... }
pub fn homefs_readdir(...) -> Result<Vec<...>, ErrorCode> { ... }
pub fn homefs_chmod(...) -> Result<(), ErrorCode> { ... }
pub fn homefs_truncate(...) -> Result<(), ErrorCode> { ... }
```

### 2. VFS Integration (kernel/src/fs.rs)

Update file operations to route to HomeFS when path starts with /home:

```rust
// In open(), check mount point:
if let Some((mount, rel, fs_type)) = resolve_mount_path(normalized_path) {
    match fs_type {
        FsType::Disk => { /* existing diskfs logic */ }
        FsType::Tmpfs => { /* existing tmpfs logic */ }
        FsType::Home => {
            // HomeFS logic
            let inode = homefs_lookup(&rel)?;
            // ... handle open flags (O_CREAT, O_TRUNC, etc.)
        }
    }
}

// Similarly update: read, write, stat, getdents64, unlink, rename, mkdir, rmdir, chmod
```

### 3. Kernel Initialization (kernel/src/main.rs)

Mount /home during boot:

```rust
// After mounting /tmp:
mount::mount_home_at_home().expect("Failed to mount /home");
println!("/home mounted (persistent filesystem)");
```

### 4. QEMU Configuration

Update kernel/Cargo.toml to add second disk:

```toml
[package.metadata.bootimage]
run-args = [
    "-serial", "stdio", 
    "-display", "none",
    "-drive", "file=fs.img,format=raw,if=ide",
    "-drive", "file=home.img,format=raw,if=ide",  # ADD THIS (slave drive)
]
test-args = [
    "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04", 
    "-serial", "stdio", 
    "-display", "none",
    "-drive", "file=fs.img,format=raw,if=ide",
    "-drive", "file=home.img,format=raw,if=ide",  # ADD THIS
]
```

### 5. Makefile Updates

Update build targets to create home.img:

```makefile
home.img:
	@echo "Generating home filesystem image..."
	python3 scripts/mkhomefs.py home.img
	@echo "Home filesystem image created!"

# Update dependencies
run: bootimage fs.img home.img
	@echo "Starting QEMU (headless mode)..."
	cargo run --manifest-path kernel/Cargo.toml --target x86_64-unknown-none

run-gui: bootimage fs.img home.img
	@echo "Starting QEMU with GUI..."
	qemu-system-x86_64 \
		-drive format=raw,file=target/x86_64-unknown-none/debug/bootimage-panda-kernel.bin \
		-drive file=fs.img,format=raw,if=ide \
		-drive file=home.img,format=raw,if=ide \
		-serial stdio
```

### 6. Error Code Mapping

Add HomeFsError -> ErrorCode conversion:

```rust
// In mount.rs
fn homefs_error_to_errno(err: HomeFsError) -> ErrorCode {
    match err {
        HomeFsError::NotFound => ErrorCode::ENOENT,
        HomeFsError::AlreadyExists => ErrorCode::EEXIST,
        HomeFsError::NotDirectory => ErrorCode::ENOTDIR,
        HomeFsError::IsDirectory => ErrorCode::EISDIR,
        HomeFsError::NoSpace => ErrorCode::ENOSPC,
        HomeFsError::InvalidArgument => ErrorCode::EINVAL,
        HomeFsError::NotEmpty => ErrorCode::ENOTEMPTY,
        HomeFsError::IoError | HomeFsError::Corrupted => ErrorCode::EIO,
    }
}
```

### 7. Cross-Device Checks

Ensure rename across filesystems returns EXDEV:

```rust
// In rename syscall handler:
let old_mount = resolve_mount_path(old_path);
let new_mount = resolve_mount_path(new_path);

match (old_mount, new_mount) {
    (Some((_, _, fs1)), Some((_, _, fs2))) if fs1 != fs2 => {
        return Err(ErrorCode::EXDEV);  // Cross-device not allowed
    }
    // ... handle same-device rename
}
```

### 8. Persistence Test (scripts/qemu-test.sh)

Add HOME_PERSIST_SMOKE test mode:

```bash
if [ "${HOME_PERSIST_SMOKE:-0}" -eq 1 ]; then
    TEST_NAME="home_persist_smoke"
    FEATURES+=(--features home-persist-smoke)
    EXPECTED_MARKER="TEST PASS home_persist_smoke"
    
    # Build once
    cargo bootimage ...
    
    # Create/find home.img (persistent across boots)
    PERSIST_HOME_IMG="target/qemu/home_persist.img"
    if [ ! -f "$PERSIST_HOME_IMG" ]; then
        python3 scripts/mkhomefs.py "$PERSIST_HOME_IMG"
    fi
    
    # BOOT 1: Create files
    echo "=== Boot 1: Creating files ===" 
    qemu-system-x86_64 \
        -drive format=raw,file="$KERNEL_IMAGE" \
        -drive file=fs.img,format=raw,if=ide \
        -drive file="$PERSIST_HOME_IMG",format=raw,if=ide \
        ... > target/qemu/home_persist_boot1.log
    
    # BOOT 2: Verify persistence
    echo "=== Boot 2: Verifying persistence ==="
    qemu-system-x86_64 \
        -drive format=raw,file="$KERNEL_IMAGE" \
        -drive file=fs.img,format=raw,if=ide \
        -drive file="$PERSIST_HOME_IMG",format=raw,if=ide \
        ... > target/qemu/home_persist_boot2.log
    
    # Check for TEST PASS in boot2 log
    if grep -q "$EXPECTED_MARKER" target/qemu/home_persist_boot2.log; then
        echo "✓ Persistence test PASSED"
        exit 0
    else
        echo "✗ Persistence test FAILED"
        exit 1
    fi
fi
```

### 9. Smoke Test Implementation (kernel/tests/)

Create home_persist_smoke.rs:

```rust
#[cfg(feature = "home-persist-smoke")]
mod home_persist_tests {
    use super::*;

    #[test_case]
    fn home_persist_smoke() {
        // Detect boot number by checking if /home/hello.txt exists
        let is_boot2 = fs::stat("/home/hello.txt").is_ok();
        
        if !is_boot2 {
            // BOOT 1: Create files
            fs::mkdir("/home/bin", 0o755).unwrap();
            
            // Copy a binary (use syscalls to copy /mnt/bin/echo)
            let data = fs::read_file_to_vec("/mnt/bin/echo").unwrap();
            fs::write_file("/home/bin/echo", &data).unwrap();
            fs::chmod("/home/bin/echo", 0o755).unwrap();
            
            // Write test file
            fs::write_file("/home/hello.txt", b"firstboot\n").unwrap();
            
            serial_println!("Boot 1 complete");
            exit_qemu(QemuExitCode::Success);
        } else {
            // BOOT 2: Verify and append
            let content = fs::read_file_to_vec("/home/hello.txt").unwrap();
            assert!(content.starts_with(b"firstboot"));
            
            // Verify binary exists
            assert!(fs::stat("/home/bin/echo").is_ok());
            
            // Append
            fs::append_file("/home/hello.txt", b"secondboot\n").unwrap();
            
            // Verify both lines
            let content = fs::read_file_to_vec("/home/hello.txt").unwrap();
            assert!(content.contains(&b"firstboot"[..]));
            assert!(content.contains(&b"secondboot"[..]));
            
            serial_println!("TEST PASS home_persist_smoke");
            exit_qemu(QemuExitCode::Success);
        }
    }
}
```

### 10. Userland Commands (Optional)

Add simple commands in userland/src/:

**df.rs**:
```rust
// Show filesystem stats
let (free, total, _, _) = syscall_statfs("/home");
println!("/home: {} free / {} total blocks", free, total);
```

**install.rs**:
```rust
// Copy file to /home/bin and make executable
let src = args[1];
let dest = format!("/home/bin/{}", basename(src));
copy_file(src, &dest);
chmod(&dest, 0o755);
```

## Testing Checklist

- [ ] Kernel builds with home.img
- [ ] /home mounts successfully at boot
- [ ] Can create files in /home
- [ ] Can write to files in /home
- [ ] Can read from files in /home
- [ ] Files persist across reboot
- [ ] Can create directories in /home
- [ ] Can delete files/directories
- [ ] Can rename within /home
- [ ] Cannot rename across devices (returns EXDEV)
- [ ] Proper error codes (ENOSPC, ENOENT, etc.)
- [ ] home_persist_smoke test passes
- [ ] No kernel panics
- [ ] Quality gate passes (fmt, clippy, tests)

## Known Limitations

1. Max file size: 4KB (8 direct blocks)
2. Max filesystem size: 512KB
3. Max files: 256
4. No journaling (requires clean shutdown)
5. No indirect blocks (limits file size)
6. No timestamps
7. No fsck/repair tool
8. Single-threaded access (no locking between cores)

## Future Enhancements

1. Increase direct blocks to 64 (32KB files)
2. Add indirect blocks (larger files)
3. Add timestamps (atime/mtime/ctime)
4. Add journaling or COW
5. Add fsck tool
6. Optimize block allocation (best-fit, clustering)
7. Add block cache for performance
8. Support larger filesystems (> 512KB)
