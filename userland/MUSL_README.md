# External Userland Programs

This directory contains test programs compiled from C source code to validate PandaOS's Linux ABI compatibility.

## Building Programs

### Using the build script:

```bash
./build_musl.sh
```

This script will detect and use either:
- `x86_64-linux-musl-gcc` (preferred)
- `musl-gcc` (fallback)
- `gcc` with `-static -nostdlib` (minimal functionality)

### Manual compilation:

#### With musl (recommended):
```bash
x86_64-linux-musl-gcc -static -nostdlib -o program program.c
```

#### With regular gcc (limited):
```bash
gcc -static -nostdlib -o program program.c
```

## Test Programs

### hello_musl.c
Simple "Hello World" program that tests:
- Direct syscall invocation (write, exit)
- String output to stdout
- Clean process termination

### true.c
Minimal implementation of the `true` command:
- Immediately exits with status 0
- Tests basic process lifecycle

### echo.c
Simple implementation of the `echo` command:
- Parses argc/argv
- Writes arguments to stdout
- Tests argument passing via execve

## Testing on PandaOS

1. Build the programs using the build script
2. Copy binaries to the disk image at `/mnt/bin/`
3. Boot PandaOS
4. Run the programs from the shell:
   ```
   /mnt/bin/hello_musl
   /mnt/bin/true
   /mnt/bin/echo hello world
   ```

## ABI Compatibility

These programs use direct syscalls to test PandaOS's Linux x86_64 syscall ABI:

- Syscall numbers match Linux (write=1, exit=60)
- Registers follow Linux convention (rax=syscall number, rdi/rsi/rdx=args)
- Return values in rax (negative for errors)

See `/ABI.md` for complete ABI documentation.

## Future Enhancements

- [ ] Add argv/envp parsing test
- [ ] Add more coreutils implementations (cat, ls, etc.)
- [ ] Add syscall error handling tests
- [ ] Add syscall fuzzing program
