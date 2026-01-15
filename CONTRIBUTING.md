# Contributing to PandaOS

Thank you for your interest in contributing to PandaOS! This document outlines the development process and standards.

## Development Setup

### Prerequisites

- Rust nightly toolchain (automatically installed via `rust-toolchain.toml`)
- QEMU for testing (optional but recommended)
- Git

### Getting Started

```bash
git clone https://github.com/armper/PandaOS.git
cd PandaOS
make install-deps
make test
```

## Code Quality Standards

PandaOS enforces strict quality gates to maintain code safety and consistency.

### Before Committing

**Always run the quality gate:**

```bash
./scripts/quality-gate.sh
```

This checks:
1. Code formatting (`cargo fmt --check`)
2. Clippy lints (`cargo clippy -- -D warnings`)
3. Host unit tests
4. Unsafe code placement

### Safety Rules

#### 1. No Unsafe Outside arch_x86_64 + Drivers

Unsafe code is only allowed in:
- `hal/src/serial.rs` (hardware driver)
- `hal/src/vga.rs` (hardware driver)
- Future arch-specific modules

**Every unsafe block MUST have a SAFETY comment:**

```rust
// SAFETY: The VGA buffer is mapped at 0xb8000 by the bootloader.
// This address is guaranteed to be valid for VGA text mode operations.
let buffer = unsafe { &mut *(0xb8000 as *mut Buffer) };
```

#### 2. No Globals for Core Subsystems

Core subsystems (scheduler, VFS, VM) must use explicit initialization:

```rust
// ❌ BAD
static mut SCHEDULER: Option<Scheduler> = None;

// ✅ GOOD
pub struct Kernel {
    scheduler: Scheduler,
    vfs: VirtualFileSystem,
}

impl Kernel {
    pub fn new() -> Self {
        Self {
            scheduler: Scheduler::new(),
            vfs: VirtualFileSystem::new(),
        }
    }
}
```

#### 3. No Allocation Before Heap Init

The kernel must panic if allocation is attempted before heap initialization:

```rust
#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

static HEAP_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init_heap() {
    // Initialize heap...
    HEAP_INITIALIZED.store(true, Ordering::SeqCst);
}
```

### Testing Requirements

#### Every Subsystem Must Have

1. **Host Unit Tests** (if possible)
   - Test pure logic on the host
   - No hardware dependencies
   - Located in `#[cfg(test)] mod tests`

2. **QEMU Integration Test** (at least one)
   - Test actual kernel behavior
   - Print `TEST PASS <name>` or `TEST FAIL <name>`
   - Use `#[test_case]` attribute

3. **Doc Comments with Invariants**
   ```rust
   /// Frame allocator for physical memory.
   ///
   /// ## Invariants
   ///
   /// - Frames are never double-allocated
   /// - Deallocated frames are returned to the pool
   /// - Frame numbers are always within the valid range
   pub struct FrameAllocator { ... }
   ```

### Code Style

- Follow Rust 2021 edition conventions
- Maximum line length: 100 characters
- Use 4 spaces for indentation (enforced by rustfmt)
- Document all public APIs
- Use descriptive variable names

### Commit Messages

Use conventional commit format:

```
feat: Add frame allocator with bitmap tracking
fix: Correct VGA buffer scrolling behavior
docs: Update architecture documentation
test: Add unit tests for PID allocator
refactor: Extract serial port driver
```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/my-feature
```

### 2. Make Changes

- Write code following the style guide
- Add tests for new functionality
- Document public APIs

### 3. Test Locally

```bash
# Run quality gate
./scripts/quality-gate.sh

# Run specific tests
make test-hal
make test-kernel

# Run in QEMU (when available)
./scripts/qemu-test.sh
```

### 4. Commit Changes

```bash
git add .
git commit -m "feat: Add my feature"
```

### 5. Push and Create PR

```bash
git push origin feature/my-feature
```

Then create a Pull Request on GitHub.

## Definition of Done

A feature is complete when:

- [ ] Code compiles without warnings
- [ ] Quality gate passes (`./scripts/quality-gate.sh`)
- [ ] Unit tests added for pure logic
- [ ] QEMU integration test added (if applicable)
- [ ] All unsafe blocks have SAFETY comments
- [ ] Documentation updated
- [ ] ARCHITECTURE.md updated (for major changes)
- [ ] Boots in QEMU successfully
- [ ] No new unsafe outside allowed modules

## Architecture Changes

For changes affecting the overall architecture:

1. Discuss in an issue first
2. Update `ARCHITECTURE.md`
3. Get review from maintainers
4. Ensure backward compatibility (or document breaking changes)

## Getting Help

- Open an issue for bugs or feature requests
- Check `ARCHITECTURE.md` for design decisions
- Review existing code for patterns and examples

## License

By contributing to PandaOS, you agree that your contributions will be licensed under the GPL-3.0 License.
