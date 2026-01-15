# PandaOS Userland Programs

This directory contains simple userland programs that run in ring 3 on PandaOS.

## Programs

### hello
A minimal "Hello World" program that demonstrates:
- System call interface (write, exit)
- Static ELF64 binary format
- User mode execution

## Building

Requirements:
- NASM (Netwide Assembler)
- GNU ld or lld (linker)

```bash
./build.sh
```

This creates userland ELF binaries under `build/` and copies them into `bin/`.
The kernel embeds the prebuilt binaries from `bin/` by default.

To rebuild userland during kernel builds, enable the feature:

```bash
cd kernel
cargo build --features build-userland
```

## Binary Format

Programs are built as flat binaries (not standard ELF) for simplicity:
- No dynamic linking
- No relocations
- Position-independent code (PIC) with relative addressing
- Direct syscalls via `int 0x80` (simplified for now)

## Syscalls

Following Linux x86_64 calling convention:

**write(fd, buf, len)** - syscall #1
- rax = 1
- rdi = file descriptor
- rsi = buffer pointer
- rdx = byte count
- Returns: bytes written in rax

**exit(status)** - syscall #60
- rax = 60
- rdi = exit code
- Does not return

## Memory Layout

User programs are loaded at:
- Entry point: 0x400000
- Data/rodata follows code
- Stack: 0x7FFFFFFFFFFF (grows down)

## Testing

Userland programs are tested in QEMU as part of kernel integration tests.
