# PandaOS Troubleshooting Guide

This guide covers common issues when building, running, or debugging PandaOS in QEMU.

## Table of Contents

- [Build Issues](#build-issues)
- [QEMU Boot Issues](#qemu-boot-issues)
- [Display Issues](#display-issues)
- [Serial Output Issues](#serial-output-issues)
- [Boot Failures](#boot-failures)
- [Debugging Tips](#debugging-tips)

---

## Build Issues

### Missing bootimage tool

**Symptom:**
```
Error: No bootimage found!
```

**Solution:**
Install the `bootimage` tool:
```bash
cargo install bootimage --version '^0.10'
make bootimage
```

### Rust toolchain issues

**Symptom:**
```
error: target 'x86_64-unknown-none' not found
```

**Solution:**
The project uses `rust-toolchain.toml` which should automatically install the correct toolchain. If it doesn't work:
```bash
rustup component add rust-src llvm-tools-preview
```

### Build fails with clippy warnings

**Symptom:**
```
error: ... [-D warnings]
```

**Solution:**
Run the quality gate to see all issues:
```bash
./scripts/quality-gate.sh
```

Fix formatting and clippy issues:
```bash
make fmt
make clippy
```

---

## QEMU Boot Issues

### QEMU not found

**Symptom:**
```
Error: qemu-system-x86_64 not found
```

**Solution:**

**macOS:**
```bash
brew install qemu
```

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get install qemu-system-x86
```

**Linux (Fedora/RHEL):**
```bash
sudo dnf install qemu-system-x86
```

### Filesystem image missing

**Symptom:**
```
Warning: fs.img not found, creating it...
```

**Solution:**
The script will try to create `fs.img` automatically using `scripts/mkdiskimg.py`. If this fails, create it manually:
```bash
python3 scripts/mkdiskimg.py
```

---

## Display Issues

### Black VGA screen but serial works

**Symptom:**
- QEMU window shows black screen
- Serial output (in terminal or log file) shows boot messages

**Possible Causes:**
1. VGA console not enabled
2. VGA driver not initialized
3. VGA writes happening but not visible

**Solutions:**

1. **Enable VGA console feature:**
   ```bash
   # Build with VGA console enabled
   cargo build --manifest-path kernel/Cargo.toml \
               --target x86_64-unknown-none \
               --features vga-console
   
   # Then create bootimage and run
   make bootimage
   GUI_VGA=1 ./scripts/run-qemu.sh
   ```

2. **Verify VGA initialization:**
   Check that `panda_hal::vga::init()` is called early in boot (it should be in `panda_hal::init()`).

3. **Use serial for debugging:**
   If VGA doesn't work, you can always use serial output:
   ```bash
   SERIAL_STDIO=1 ./scripts/run-qemu.sh
   ```

### VGA shows garbled text

**Symptom:**
- VGA window shows random characters or symbols

**Possible Causes:**
- Memory corruption
- Wrong VGA buffer address
- Character encoding issues

**Solutions:**
1. Check boot diagnostics for panic messages
2. Enable serial output to see detailed logs
3. Verify VGA buffer is mapped at correct address (0xb8000)

---

## Serial Output Issues

### No serial output at all

**Symptom:**
- Neither terminal nor log file shows any output
- QEMU window may or may not show anything

**Possible Causes:**
1. Wrong serial port configuration
2. Serial not initialized early enough
3. QEMU args incorrect

**Solutions:**

1. **Verify QEMU serial arguments:**
   ```bash
   # For output in terminal:
   SERIAL_STDIO=1 ./scripts/run-qemu.sh
   
   # For output to file:
   GUI_VGA=1 ./scripts/run-qemu.sh
   # Then check: target/qemu/run.log
   ```

2. **Check serial port address:**
   PandaOS uses COM1 (0x3F8). Verify in `hal/src/serial.rs` that init uses:
   ```rust
   SerialPort::new(0x3F8)  // COM1
   ```

3. **Verify early initialization:**
   Serial should be initialized in `_start()` before any print statements.

### Serial output stops mid-boot

**Symptom:**
- Serial output shows some boot messages
- Then stops at a specific BOOT STEP

**Possible Causes:**
- Kernel panic occurred
- Deadlock or infinite loop
- Triple fault (CPU reset)

**Solutions:**
1. Note the last BOOT STEP number shown
2. Check the panic handler output
3. Enable QEMU logging:
   ```bash
   QEMU_ARGS="-d int,cpu_reset" SERIAL_STDIO=1 ./scripts/run-qemu.sh
   ```

---

## Boot Failures

### Stuck at early BOOT STEP (1-5)

**Symptom:**
```
BOOT STEP 3 cpu=0 cr3=0x... rsp=0x...
```
Then nothing.

**Possible Issues:**
- Memory initialization failed
- Paging setup error
- GDT/IDT problems

**Debug Steps:**
1. Check which step it stopped at:
   - STEP 1-2: HAL or memory init issue
   - STEP 3: Paging or interrupt setup
   - STEP 4-5: GDT or interrupt handling

2. Look for panic messages in serial output

3. Run with full QEMU logging:
   ```bash
   QEMU_ARGS="-d int,cpu_reset,guest_errors" SERIAL_STDIO=1 ./scripts/run-qemu.sh 2>&1 | tee boot.log
   ```

### Stuck after "PANDA READY"

**Symptom:**
- Boot completes successfully
- "PANDA READY" marker appears
- Shell prompt never shows

**Possible Causes:**
1. Init process not found
2. Init process failed to exec /bin/sh
3. Shell binary missing or corrupt

**Solutions:**

1. **Verify init exists:**
   ```bash
   # Check if init is in filesystem image
   python3 scripts/mkdiskimg.py
   ```

2. **Check init path:**
   The kernel looks for init at:
   - `/mnt/bin/init` (disk filesystem)
   - `/init` (fallback)

3. **Verify shell exists:**
   ```bash
   ls -la userland/bin/sh
   ```
   
   If missing, build userland:
   ```bash
   cd userland && ./build.sh
   ```

### No "PANDA READY" marker appears

**Symptom:**
- Boot messages appear
- Scheduler starts
- But no "PANDA READY" marker

**Possible Causes:**
- Old kernel binary (before PANDA READY was added)
- Kernel panicked before reaching that point

**Solutions:**
1. Rebuild kernel:
   ```bash
   make clean
   make build
   make bootimage
   ```

2. Check for panic messages in serial output

3. Verify boot reaches BOOT STEP 11:
   ```
   BOOT STEP 11 cpu=0 cr3=0x... rsp=0x...
   ```

### Kernel panic on boot

**Symptom:**
```
╔════════════════════════════════════════════════════════════════╗
║                      KERNEL PANIC                              ║
╚════════════════════════════════════════════════════════════════╝

Panic: ...
CPU ID: 0
CR3:    0x...
RSP:    0x...
```

**Debug Steps:**

1. **Read the panic message:**
   - Look for the specific error (e.g., "Failed to initialize identity mapping")
   - Note the file and line number if provided

2. **Check boot diagnostics:**
   ```
   === Boot Diagnostics ===
   Last N boot steps:
     [0] step 1
     [1] step 2
     ...
   ```
   This shows how far boot progressed.

3. **Check register values:**
   - CR3: Page table physical address
   - RSP: Stack pointer (should be in kernel stack range)
   - If RSP is very low or very high, stack overflow/underflow

4. **Common panic causes:**
   - "Allocation error": Heap exhausted or not initialized
   - "Page fault": Invalid memory access
   - "Failed to map heap": Memory allocator issue
   - "init program not found": Filesystem not mounted correctly

---

## Debugging Tips

### Enable verbose boot output

All boot messages go to serial. To see them:
```bash
SERIAL_STDIO=1 ./scripts/run-qemu.sh 2>&1 | tee boot.log
```

### Check BOOT STEP markers

Each BOOT STEP corresponds to a phase:
- **1**: HAL initialization (serial, VGA)
- **2**: Memory subsystem
- **3**: Paging infrastructure
- **4**: GDT initialization
- **5**: Interrupt handling
- **6**: Syscall/sysret support
- **7**: Heap region mapping
- **8**: Heap allocator init
- **9**: (reserved)
- **10**: Mount table and filesystems
- **11**: Scheduler start

If boot stops at a particular step, that subsystem likely failed.

### Run in multiple modes

Try different display modes to isolate issues:

```bash
# Serial only (good for debugging)
SERIAL_STDIO=1 ./scripts/run-qemu.sh

# GUI only (see if VGA works)
GUI_VGA=1 ./scripts/run-qemu.sh

# Both (compare outputs)
BOTH=1 ./scripts/run-qemu.sh
```

### Use QEMU debug features

```bash
# Enable CPU reset logging
QEMU_ARGS="-d cpu_reset" SERIAL_STDIO=1 ./scripts/run-qemu.sh

# Enable interrupt logging
QEMU_ARGS="-d int" SERIAL_STDIO=1 ./scripts/run-qemu.sh

# Enable guest errors
QEMU_ARGS="-d guest_errors" SERIAL_STDIO=1 ./scripts/run-qemu.sh

# Combine multiple
QEMU_ARGS="-d int,cpu_reset,guest_errors" SERIAL_STDIO=1 ./scripts/run-qemu.sh
```

### Check boot diagnostics

When a panic occurs, the kernel dumps:
- CPU ID
- Control registers (CR3)
- Stack pointer (RSP)
- Last N boot steps

Use this information to pinpoint where boot failed.

### Run integration tests

The test suite can help validate subsystems:
```bash
# Run all tests
make test

# Run kernel-specific tests
make test-kernel

# Run HAL tests
make test-hal
```

### Compare with known-good build

If boot suddenly breaks:
1. Check git history for recent changes
2. Try building an older commit that worked
3. Bisect to find the breaking change

---

## Getting Help

If you're still stuck:

1. **Collect diagnostic information:**
   ```bash
   # Full boot log
   SERIAL_STDIO=1 ./scripts/run-qemu.sh 2>&1 | tee boot-debug.log
   
   # Build information
   cargo --version
   rustc --version
   
   # System information
   uname -a
   qemu-system-x86_64 --version
   ```

2. **Check for panic messages** in the output

3. **Note the last BOOT STEP** reached

4. **Check the GitHub issues** for similar problems

5. **Open an issue** with:
   - Full boot log
   - Last BOOT STEP reached
   - Panic message (if any)
   - Build/system information
   - Steps to reproduce

---

## Quick Reference

### Run Modes

| Mode | Command | Description |
|------|---------|-------------|
| Serial stdio | `SERIAL_STDIO=1 ./scripts/run-qemu.sh` | Terminal shows serial output (default) |
| GUI VGA | `GUI_VGA=1 ./scripts/run-qemu.sh` | QEMU window shows VGA text |
| Both | `BOTH=1 ./scripts/run-qemu.sh` | VGA window + serial in terminal |
| Headless | `HEADLESS=1 ./scripts/run-qemu.sh` | No display, serial only |

### Common Commands

```bash
# Build everything
make build

# Create bootimage
make bootimage

# Run with GUI
make run-gui

# Run tests
make test

# Format and lint
make fmt
make clippy

# Clean build
make clean
```

### Boot Success Indicators

1. Serial output shows:
   ```
   [BOOT] serial ok
   ╔════════════════════════════════════════════════════════════════╗
   ║              PandaOS - Unix-like x86_64 Kernel                 ║
   ║                    Version ...                                 ║
   ╚════════════════════════════════════════════════════════════════╝
   ```

2. BOOT STEP markers appear sequentially (1-11)

3. "PANDA READY" marker appears:
   ```
   ════════════════════════════════════════════════════════════════
                           PANDA READY
   ════════════════════════════════════════════════════════════════
   ```

4. Shell prompt appears:
   ```
   panda>
   ```

If you see all four, boot was successful!
