; touch - create empty file
; Opens/creates a file and immediately closes it

BITS 64

%define SYS_OPEN 2
%define SYS_CLOSE 3
%define SYS_WRITE 1
%define SYS_EXIT 60

%define STDOUT 1
%define STDERR 2

%define O_RDONLY 0x0000
%define O_WRONLY 0x0001
%define O_CREAT 0x0040

%define ARG_ADDR 0x7FFFFFFFC000

section .text
global _start

_start:
    ; Open file with O_CREAT | O_WRONLY
    mov rax, SYS_OPEN
    mov rdi, ARG_ADDR       ; path from execve
    mov rsi, O_WRONLY | O_CREAT
    mov rdx, 0o644          ; mode
    syscall
    
    ; Check if open succeeded
    test rax, rax
    js error
    
    ; Close the file
    mov rdi, rax            ; fd
    mov rax, SYS_CLOSE
    syscall
    
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
err_msg: db "touch: failed to create file", 10
err_msg_len: equ $ - err_msg
