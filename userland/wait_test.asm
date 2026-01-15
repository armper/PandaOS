; wait_test.asm - Test blocking waitpid functionality
; This program forks a child, the child exits immediately,
; and the parent waits for it without busy looping.

BITS 64

section .text
global _start

; Syscall numbers
SYS_WRITE  equ 1
SYS_EXIT   equ 60
SYS_FORK   equ 57
SYS_WAIT4  equ 61

; File descriptors
STDOUT     equ 1

_start:
    ; Write "wait_test: forking child"
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, msg_fork
    mov rdx, msg_fork_len
    syscall

    ; Fork a child process
    mov rax, SYS_FORK
    syscall

    ; Check if we're parent or child
    test rax, rax
    js .fork_failed
    jz .child_process

.parent_process:
    ; Save child PID
    push rax

    ; Write "wait_test: parent waiting"
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, msg_parent_wait
    mov rdx, msg_parent_wait_len
    syscall

    ; Wait for child (blocking call)
    pop rsi              ; child PID
    mov rax, SYS_WAIT4
    mov rdi, -1          ; wait for any child
    xor rsi, rsi         ; status pointer (NULL)
    xor rdx, rdx         ; options (0)
    syscall

    ; Check waitpid result
    test rax, rax
    js .wait_failed

    ; Write "wait_test: parent resumed"
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, msg_parent_resume
    mov rdx, msg_parent_resume_len
    syscall

    ; Write success message
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, msg_success
    mov rdx, msg_success_len
    syscall

    ; Exit parent with status 0
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

.child_process:
    ; Write "wait_test: child running"
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, msg_child
    mov rdx, msg_child_len
    syscall

    ; Exit child immediately with status 42
    mov rax, SYS_EXIT
    mov rdi, 42
    syscall

.fork_failed:
    ; Write "wait_test: fork failed"
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, msg_fork_fail
    mov rdx, msg_fork_fail_len
    syscall

    ; Exit with error
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

.wait_failed:
    ; Write "wait_test: wait failed"
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, msg_wait_fail
    mov rdx, msg_wait_fail_len
    syscall

    ; Exit with error
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

section .rodata
msg_fork:          db "wait_test: forking child", 10
msg_fork_len:      equ $ - msg_fork

msg_parent_wait:   db "wait_test: parent waiting (blocked)", 10
msg_parent_wait_len: equ $ - msg_parent_wait

msg_child:         db "wait_test: child running", 10
msg_child_len:     equ $ - msg_child

msg_parent_resume: db "wait_test: parent resumed after wait", 10
msg_parent_resume_len: equ $ - msg_parent_resume

msg_success:       db "wait_test: TEST PASS", 10
msg_success_len:   equ $ - msg_success

msg_fork_fail:     db "wait_test: fork failed", 10
msg_fork_fail_len: equ $ - msg_fork_fail

msg_wait_fail:     db "wait_test: wait failed", 10
msg_wait_fail_len: equ $ - msg_wait_fail
