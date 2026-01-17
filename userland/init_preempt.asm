; init_preempt.asm - Init program for preemption smoke test
; This program runs the preempt_test program and exits

bits 64

; Syscall numbers
SYS_WRITE  equ 1
SYS_EXECVE equ 59
SYS_EXIT   equ 60

section .data
    msg: db 'Starting preemption test...', 10
    msg_len: equ $ - msg
    path: db '/mnt/bin/preempt_test', 0
    argv: dq path, 0      ; argv[0] = path, argv[1] = NULL
    envp: dq 0            ; envp[0] = NULL

section .text
global _start

_start:
    ; Print start message
    mov rax, SYS_WRITE
    mov rdi, 1              ; stdout
    lea rsi, [rel msg]
    mov rdx, msg_len
    syscall

    ; Execute preempt_test
    mov rax, SYS_EXECVE
    lea rdi, [rel path]
    lea rsi, [rel argv]
    lea rdx, [rel envp]
    syscall

    ; If execve fails, exit
    mov rax, SYS_EXIT
    mov rdi, 1              ; exit code 1 (failure)
    syscall
