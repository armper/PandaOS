; /bin/wc - Word count (byte count only for now)
; Reads from stdin until EOF and prints byte count

BITS 64

%define SYS_READ 0
%define SYS_WRITE 1
%define SYS_EXIT 60

%define STDIN 0
%define STDOUT 1
%define STDERR 2

%define BUF_SIZE 1024

section .text
global _start

_start:
    xor r12, r12        ; byte counter

read_loop:
    ; Read from stdin
    mov rax, SYS_READ
    mov rdi, STDIN
    lea rsi, [rel buffer]
    mov rdx, BUF_SIZE
    syscall
    
    ; Check for error or EOF
    test rax, rax
    jle read_done
    
    ; Add to byte count
    add r12, rax
    jmp read_loop

read_done:
    ; Convert count to decimal string
    mov rax, r12
    lea rdi, [rel count_str_end]
    mov rcx, 10
    
convert_loop:
    xor rdx, rdx
    div rcx
    add dl, '0'
    dec rdi
    mov [rdi], dl
    test rax, rax
    jnz convert_loop
    
    ; Write the count
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, rdi        ; rdi still points to start of number
    lea rdx, [rel count_str_end]
    sub rdx, rsi
    syscall
    
    ; Write newline
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, newline_len
    syscall
    
    ; Exit successfully
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

section .bss
buffer: resb BUF_SIZE

section .data
count_str: times 20 db ' '
count_str_end:
newline: db 0x0D, 0x0A
newline_len equ $ - newline
