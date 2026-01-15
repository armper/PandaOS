# PandaOS Userland Programs

This directory contains simple userland programs that run in ring 3 on PandaOS.

## Programs

### /bin/sh
Interactive shell with fork/exec/wait support:
- Builtins: `help`, `echo` (builtin), `exit`
- External commands: `cat`, `true`, `echo`, `wc`
- Line editing with backspace support
- Fork/exec/wait for external programs
- **NEW**: Single-pipe pipeline support (`cmd1 | cmd2`)

### /bin/cat
Read and display file contents:
- Usage: `cat <path>`
- Reads from in-memory VFS
- Example: `cat /etc/version`

### /bin/echo
Echo arguments to stdout:
- Usage: `echo <text>`
- Reads argument from kernel-provided exec arg buffer
- Writes text + newline to stdout
- Example: `echo hello`
- Useful in pipelines: `echo hello | wc`

### /bin/wc
Word count (byte count only):
- Usage: `wc` (reads from stdin)
- Counts bytes read until EOF
- Prints decimal byte count + newline
- Example: `echo hello | wc` outputs `6` (5 chars + newline)
- Useful for testing pipes

### /bin/true
Minimal program that exits with status 0:
- Used for testing fork/exec/wait
- No arguments or output

### /bin/hello, /bin/hello1, /bin/hello2
Simple "Hello World" variants demonstrating:
- System call interface (write, exit)
- Static ELF64 binary format
- User mode execution

### /bin/init
First process started by kernel:
- PID 1
- Execs `/bin/sh` on startup

## Building

### Prerequisites
- NASM (Netwide Assembler) - `sudo apt-get install nasm`
- GNU ld or lld (linker) - usually comes with build-essential

### Rebuild All Binaries
```bash
cd userland
./build.sh
```

This assembles `.asm` files, links them into ELF executables in `build/`, and copies them to `bin/`.

**Output:**
- `build/*.o` - Object files (gitignored)
- `build/*` - Linked ELF executables (gitignored)
- `bin/*` - Final binaries committed to repo

### Rebuild During Kernel Build
To automatically rebuild userland binaries during kernel compilation:

```bash
cd kernel
cargo build --features build-userland
```

This requires `nasm` and a linker to be installed. If tools are missing, the build will fail with a clear error message.

**Default behavior:** The kernel uses prebuilt binaries from `userland/bin/` without requiring `nasm`.

## Binary Format

Programs are built as statically-linked ELF64 executables:
- No dynamic linking
- No relocations needed at runtime
- Position-independent code (PIC) with relative addressing
- Direct syscalls via `syscall` instruction (x86-64 ABI)

## Syscalls

Following Linux x86_64 calling convention:

**read(fd, buf, len)** - syscall #0
- rax = 0
- rdi = file descriptor
- rsi = buffer pointer
- rdx = byte count
- Returns: bytes read in rax, or negative errno

**write(fd, buf, len)** - syscall #1
- rax = 1
- rdi = file descriptor
- rsi = buffer pointer
- rdx = byte count
- Returns: bytes written in rax

**open(path, flags, mode)** - syscall #2
- rax = 2
- rdi = path string pointer
- rsi = flags (O_RDONLY=0, O_WRONLY=1, O_RDWR=2)
- rdx = mode (unused)
- Returns: file descriptor or negative errno

**close(fd)** - syscall #3
- rax = 3
- rdi = file descriptor
- Returns: 0 on success, negative errno on error

**fork()** - syscall #57
- rax = 57
- Returns: child PID in parent, 0 in child, negative errno on error

**execve(path, arg, envp)** - syscall #59
- rax = 59
- rdi = program path string pointer
- rsi = single argument string pointer (or 0 for no arg)
- rdx = environment (unused, pass 0)
- Returns: does not return on success, negative errno on error

**exit(status)** - syscall #60
- rax = 60
- rdi = exit code
- Does not return

**wait4(pid, status, options, rusage)** - syscall #61
- rax = 61
- rdi = child PID to wait for (-1 for any child)
- rsi = pointer to store exit status (or 0 to ignore)
- rdx = options (0 for default)
- r10 = rusage (unused, pass 0)
- Returns: PID of exited child, or negative errno (EAGAIN if no zombie yet, ESRCH if no children)

**pipe(pipefd)** - syscall #22
- rax = 22
- rdi = pointer to int[2] array to receive pipe fds
- Returns: 0 on success, fills pipefd[0] with read end, pipefd[1] with write end
- Returns: negative errno on error

**dup2(oldfd, newfd)** - syscall #33
- rax = 33
- rdi = source file descriptor
- rsi = target file descriptor
- Returns: newfd on success, negative errno on error
- Used to redirect stdin/stdout/stderr to pipes

## Memory Layout

User programs are loaded at:
- Entry point: 0x400000
- Text/rodata/data sections follow
- Stack: 0x7FFFFFFFFFFF (grows down)
- Exec arg buffer: 0x7FFFFFFFC000 (128 bytes, contains argument passed to execve)
- Heap: not yet implemented

## Pipelines

The shell supports single-pipe pipelines using `|` operator:

```bash
panda> echo hello | wc
6
panda> cat /etc/motd | wc
48
```

**Implementation:**
1. Shell parses command line, detects `|`
2. Splits into left command and right command
3. Creates pipe using `pipe()` syscall
4. Forks left child:
   - Redirects stdout to pipe write end using `dup2(wfd, 1)`
   - Closes both pipe ends
   - Execs left command
5. Forks right child:
   - Redirects stdin from pipe read end using `dup2(rfd, 0)`
   - Closes both pipe ends
   - Execs right command
6. Parent:
   - Closes both pipe ends
   - Waits for both children using `wait4()`
   - Reprompts

**Limitations:**
- Only single pipe supported (no `a|b|c`)
- No quoting or escaping
- No job control

## Testing

Userland programs are tested in QEMU as part of kernel integration tests:

```bash
# Test shell with help command
SHELL_SMOKE=1 ./scripts/qemu-test.sh

# Test VFS cat command
VFS_CAT_SMOKE=1 ./scripts/qemu-test.sh

# Test fork/exec/wait with cat and true
FORK_EXEC_SMOKE=1 ./scripts/qemu-test.sh

# Test pipes with echo and wc
PIPE_SMOKE=1 ./scripts/qemu-test.sh
```
