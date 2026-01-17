# Job Control Shell Requirements

This document describes the shell changes needed to complete job control support with `fg` builtin.

## Overview

The kernel now supports:
- SIGTSTP (Ctrl+Z) to stop processes
- SIGCONT to resume stopped processes
- WUNTRACED waitpid option to detect stopped children
- Proper status encoding for stopped processes

The shell needs to be updated to:
1. Track the last stopped job
2. Handle stopped children in the wait loop
3. Implement the `fg` builtin command

## Required Shell Changes

### 1. Add stopped_pgid Variable

```asm
section .data
foreground_pgid: dq 0      ; Current foreground process group (existing)
stopped_pgid: dq 0         ; Last stopped job process group (NEW)
```

### 2. Update parent_wait Loop

Current code waits for child to exit:
```asm
parent_wait:
    mov rax, SYS_WAIT4
    mov rdi, r12              ; Wait for specific child
    xor rsi, rsi              ; Don't care about status
    xor rdx, rdx              ; options = 0
    syscall
    test rax, rax
    js parent_wait            ; If error, retry
    
    ; Child exited, clear foreground and continue
    mov qword [rel foreground_pgid], 0
    jmp main_loop
```

**Update to:**
```asm
parent_wait:
    mov rax, SYS_WAIT4
    mov rdi, r12              ; Wait for specific child  
    lea rsi, [rel wait_status] ; Get status (NEW)
    mov rdx, 2                ; options = WUNTRACED (0x2)
    syscall
    test rax, rax
    js parent_wait            ; If error, retry
    
    ; Check if child stopped or exited
    mov eax, [rel wait_status]
    mov ebx, eax
    and ebx, 0xFF
    cmp ebx, 0x7F             ; Check for stopped (status & 0xff == 0x7f)
    je child_stopped
    
    ; Child exited normally
    mov qword [rel foreground_pgid], 0
    jmp main_loop

child_stopped:
    ; Child was stopped by signal
    mov rax, r12
    mov [rel stopped_pgid], rax    ; Save stopped job pgid
    mov qword [rel foreground_pgid], 0
    
    ; Print "[stopped] <pgid>" message
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel stopped_msg]
    mov rdx, stopped_msg_len
    syscall
    
    ; Print the pgid number
    ; (simple implementation: just print fixed message)
    
    jmp main_loop
```

### 3. Add fg Builtin Command

Add check after the `check_cd` section:

```asm
check_fg:
    cmp r12, 2
    jne check_echo
    mov al, [r13]
    cmp al, 'f'
    jne check_echo
    mov al, [r13 + 1]
    cmp al, 'g'
    jne check_echo
    jmp cmd_fg

; ... (later in code)

cmd_fg:
    ; Check if we have a stopped job
    mov rax, [rel stopped_pgid]
    test rax, rax
    jz fg_no_job
    
    ; Send SIGCONT to the stopped process group
    mov rdi, rax
    neg rdi                   ; Negate to signal process group
    mov rsi, 18               ; SIGCONT = 18
    mov rax, SYS_KILL
    syscall
    
    ; Set as foreground job
    mov rax, [rel stopped_pgid]
    mov [rel foreground_pgid], rax
    mov r12, rax              ; Save pgid for wait
    mov qword [rel stopped_pgid], 0  ; Clear stopped job
    
    ; Wait for the job (it might stop again or exit)
    jmp parent_wait

fg_no_job:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel fg_no_job_msg]
    mov rdx, fg_no_job_msg_len
    syscall
    jmp main_loop
```

### 4. Add New Data Section Entries

```asm
section .data
wait_status: dd 0              ; waitpid status (NEW)
stopped_msg: db "[stopped]", 0x0A
stopped_msg_len: equ $ - stopped_msg
fg_no_job_msg: db "fg: no stopped job", 0x0A
fg_no_job_msg_len: equ $ - fg_no_job_msg
```

## Testing the Implementation

### Manual Test Sequence

1. Start the shell
2. Run a long-running command (e.g., `cat /dev/stdin` or a loop)
3. Press Ctrl+Z
4. Shell should print: `[stopped]`
5. Shell prompt should return
6. Type `fg` and press Enter
7. Process should resume
8. Press Ctrl+C to terminate

### Expected Behavior

```
$ cat
^Z
[stopped]
$ fg
(cat resumes, continues reading)
^C
$ 
```

## Integration with /bin/sleepy

The `sleepy` program (created in `userland/sleepy.asm`) is ideal for testing:
- Prints "tick" repeatedly
- Yields CPU (allows signals to be processed)
- Easy to observe stop/continue behavior

Test sequence:
```
$ sleepy
tick
tick
^Z
[stopped]
$ fg
tick
tick
^C
$ 
```

## Build Instructions

To rebuild the shell with these changes:

```bash
cd userland
nasm -f elf64 sh.asm -o build/sh.o
ld -o build/sh build/sh.o -static -nostdlib --entry=_start
cp build/sh bin/sh
```

Then rebuild the kernel to embed the updated shell binary.

## Status Encoding Reference

When `waitpid()` returns with `WUNTRACED`:
- **Exited**: `status & 0x7f == 0`, exit code = `status >> 8`
- **Stopped**: `status & 0xff == 0x7f`, signal = `status >> 8`
- **Signaled**: Other values (not used in minimal implementation)

For stopped by SIGTSTP (20):
- status = `(20 << 8) | 0x7f = 0x147f`
- Check: `status & 0xff == 0x7f` → true (stopped)
- Signal: `status >> 8 == 20` → SIGTSTP
