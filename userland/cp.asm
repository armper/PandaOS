; cp - copy file
; Copies file contents from source to destination
; Expects source at ARG_ADDR and destination immediately after (null-terminated)

BITS 64

%define SYS_READ 0
%define SYS_WRITE 1
%define SYS_OPEN 2
%define SYS_CLOSE 3
%define SYS_EXIT 60

%define STDOUT 1
%define STDERR 2

%define O_RDONLY 0x0000
%define O_WRONLY 0x0001
%define O_CREAT 0x0040
%define O_TRUNC 0x0200

%define ARG_ADDR 0x7FFFFFFFC000
%define BUF_SIZE 256

section .text
global _start

_start:
    ; Find second argument (destination)
    lea rdi, [ARG_ADDR]
    xor rcx, rcx
    dec rcx                 ; rcx = -1
    xor al, al
    repne scasb            ; find null terminator
    mov r13, rdi           ; save destination path
    
    ; Open source file
    mov rax, SYS_OPEN
    mov rdi, ARG_ADDR       ; source path
    mov rsi, O_RDONLY
    xor rdx, rdx
    syscall
    test rax, rax
    js error_open_src
    mov r12, rax            ; save source fd
    
    ; Open destination file (create/truncate)
    mov rax, SYS_OPEN
    mov rdi, r13            ; destination path
    mov rsi, O_WRONLY | O_CREAT | O_TRUNC
    mov rdx, 0o644          ; mode
    syscall
    test rax, rax
    js error_open_dst
    mov r14, rax            ; save destination fd

copy_loop:
    ; Read from source
    mov rax, SYS_READ
    mov rdi, r12
    lea rsi, [rel buf]
    mov rdx, BUF_SIZE
    syscall
    test rax, rax
    js error_read
    cmp rax, 0
    je done
    
    ; Write to destination
    mov rbx, rax            ; save bytes read
    mov rax, SYS_WRITE
    mov rdi, r14
    lea rsi, [rel buf]
    mov rdx, rbx
    syscall
    test rax, rax
    js error_write
    
    jmp copy_loop

done:
    ; Close both files
    mov rax, SYS_CLOSE
    mov rdi, r12
    syscall
    
    mov rax, SYS_CLOSE
    mov rdi, r14
    syscall
    
    ; Exit success
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

error_open_src:
    lea rsi, [rel err_src]
    mov rdx, err_src_len
    jmp print_error

error_open_dst:
    lea rsi, [rel err_dst]
    mov rdx, err_dst_len
    ; Close source fd
    push rsi
    push rdx
    mov rax, SYS_CLOSE
    mov rdi, r12
    syscall
    pop rdx
    pop rsi
    jmp print_error

error_read:
    lea rsi, [rel err_read]
    mov rdx, err_read_len
    jmp cleanup_and_error

error_write:
    lea rsi, [rel err_write]
    mov rdx, err_write_len
    jmp cleanup_and_error

cleanup_and_error:
    ; Close both files
    push rsi
    push rdx
    mov rax, SYS_CLOSE
    mov rdi, r12
    syscall
    mov rax, SYS_CLOSE
    mov rdi, r14
    syscall
    pop rdx
    pop rsi

print_error:
    ; Print error message
    mov rax, SYS_WRITE
    mov rdi, STDERR
    syscall
    
    ; Exit with error code
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

section .bss
buf: resb BUF_SIZE

section .data
err_src: db "cp: failed to open source file", 10
err_src_len: equ $ - err_src
err_dst: db "cp: failed to open destination file", 10
err_dst_len: equ $ - err_dst
err_read: db "cp: failed to read from source", 10
err_read_len: equ $ - err_read
err_write: db "cp: failed to write to destination", 10
err_write_len: equ $ - err_write
