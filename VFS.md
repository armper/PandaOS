# VFS

## Overview

PandaOS provides a tiny read-only virtual filesystem backed by an in-memory table of static
file nodes. It supports absolute-path lookup, per-process file descriptors, and sequential
reads with per-fd offsets.

## Invariants

- Paths are absolute and matched by exact string equality.
- The filesystem is read-only; no write, create, or delete operations exist.
- File data is embedded as static byte slices.
- Each process owns a fixed-size FD table (16 entries).
- FDs 0/1/2 are reserved for stdin/stdout/stderr and are not stored in the table.
- open() returns the lowest available FD >= 3.
- close(0/1/2) returns EINVAL.
- read() advances the per-fd offset and returns 0 on EOF.

## FD Semantics

- `fd 0`: stdin (serial input)
- `fd 1`: stdout (serial output)
- `fd 2`: stderr (serial output)
- `fd >= 3`: read-only files backed by the in-memory table

## Fork Behavior

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
