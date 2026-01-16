; rm - remove file
; Calls unlink syscall to delete a file

BITS 64

%define SYS_UNLINK 87
%define SYS_WRITE 1
%define SYS_EXIT 60

%define STDOUT 1
%define STDERR 2

%define ARG_ADDR 0x7FFFFFFFC000

section .text
global _start

_start:
    ; Call unlink syscall
    mov rax, SYS_UNLINK
    mov rdi, ARG_ADDR       ; path from execve
    syscall
    
    ; Check if unlink succeeded
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
err_msg: db "rm: failed to remove file", 10
err_msg_len: equ $ - err_msg
