; Minimal ls for PandaOS
; Lists directory entries using getdents64 syscall

BITS 64

%define SYS_WRITE 1
%define SYS_OPEN 2
%define SYS_CLOSE 3
%define SYS_EXIT 60
%define SYS_GETDENTS64 217

%define STDOUT 1

%define BUF_SIZE 1024

; Directory entry structure (getdents64)
; struct linux_dirent64 {
;     u64 d_ino;      // Inode number
;     u64 d_off;      // Offset to next entry
;     u16 d_reclen;   // Length of this entry
;     u8  d_type;     // File type
;     char d_name[];  // Null-terminated filename
; }

section .text
global _start

_start:
    ; Open root directory "/"
    mov rax, SYS_OPEN
    lea rdi, [rel root_path]
    xor rsi, rsi
    xor rdx, rdx
    syscall
    test rax, rax
    js open_failed
    mov r12, rax            ; r12 = directory fd

read_entries:
    ; Call getdents64
    mov rax, SYS_GETDENTS64
    mov rdi, r12            ; directory fd
    lea rsi, [rel buf]      ; buffer
    mov rdx, BUF_SIZE       ; buffer size
    syscall
    test rax, rax
    js read_failed
    cmp rax, 0
    je done                 ; EOF
    
    mov r13, rax            ; r13 = bytes read
    xor r14, r14            ; r14 = current offset in buffer

process_entry:
    cmp r14, r13
    jge read_entries        ; processed all entries, read more
    
    ; Get pointer to current entry
    lea rbx, [rel buf]
    add rbx, r14
    
    ; Get d_reclen (at offset 16)
    movzx r15, word [rbx + 16]  ; r15 = record length
    
    ; Get d_name (starts at offset 19)
    lea rsi, [rbx + 19]
    
    ; Print the name
    call print_name
    
    ; Print newline
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, 2
    syscall
    
    ; Move to next entry
    add r14, r15
    jmp process_entry

done:
    mov rax, SYS_CLOSE
    mov rdi, r12
    syscall
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

open_failed:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel open_err]
    mov rdx, open_err_len
    syscall
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

read_failed:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel read_err]
    mov rdx, read_err_len
    syscall
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

; print_name: Print null-terminated string
; Input: rsi = pointer to string
print_name:
    push rbx
    push rcx
    mov rbx, rsi
    
    ; Find string length
    xor rcx, rcx
.count:
    cmp byte [rbx + rcx], 0
    je .print
    inc rcx
    cmp rcx, 256            ; safety limit
    jl .count
    
.print:
    test rcx, rcx
    jz .done
    
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, rbx
    mov rdx, rcx
    syscall
    
.done:
    pop rcx
    pop rbx
    ret

section .rodata
root_path: db "/", 0
open_err: db "ls: open failed", 0x0D, 0x0A
open_err_len equ $ - open_err
read_err: db "ls: getdents64 failed", 0x0D, 0x0A
read_err_len equ $ - read_err
newline: db 0x0D, 0x0A

section .bss
buf: resb BUF_SIZE
