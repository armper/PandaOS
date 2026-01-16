; mkdir - create directory
; Calls mkdir syscall to create a new directory

BITS 64

%define SYS_MKDIR 83
%define SYS_WRITE 1
%define SYS_EXIT 60

%define STDOUT 1
%define STDERR 2

%define ARG_ADDR 0x7FFFFFFFC000

section .text
global _start

_start:
    ; Call mkdir syscall
    mov rax, SYS_MKDIR
    mov rdi, ARG_ADDR       ; path from execve
    mov rsi, 0o755          ; mode
    syscall
    
    ; Check if mkdir succeeded
    test rax, rax
    js error
    
    ; Exit success
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

error:
    ; Print error message
    mov rax, SYS_WRITE
    mov rdi, STDERR
    lea rsi, [rel err_msg]
    mov rdx, err_msg_len
    syscall
    
    ; Exit with error code
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

section .data
err_msg: db "mkdir: failed to create directory", 10
err_msg_len: equ $ - err_msg
