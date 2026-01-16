; mv - move/rename file
; Calls rename syscall (requires two arguments)
; For now, expects old path at ARG_ADDR and new path immediately after (null-terminated)

BITS 64

%define SYS_RENAME 82
%define SYS_WRITE 1
%define SYS_EXIT 60

%define STDOUT 1
%define STDERR 2

%define ARG_ADDR 0x7FFFFFFFC000

section .text
global _start

_start:
    ; Find second argument (skip first null-terminated string)
    lea rdi, [ARG_ADDR]
    xor rcx, rcx
    dec rcx                 ; rcx = -1
    xor al, al
    repne scasb            ; find null terminator
    
    ; Now rdi points to second argument
    ; Call rename syscall
    mov rax, SYS_RENAME
    mov rsi, rdi            ; new path (second arg)
    mov rdi, ARG_ADDR       ; old path (first arg)
    syscall
    
    ; Check if rename succeeded
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
err_msg: db "mv: failed to rename file", 10
err_msg_len: equ $ - err_msg
