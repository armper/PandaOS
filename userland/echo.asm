; /bin/echo - Echo arguments to stdout
; Usage: echo <text>

BITS 64

%define SYS_READ 0
%define SYS_WRITE 1
%define SYS_EXIT 60

%define STDIN 0
%define STDOUT 1
%define STDERR 2

; Exec argument location (fixed by kernel convention)
%define EXEC_ARG_ADDR 0x7FFFFFFFC000
%define EXEC_ARG_MAX 128

section .text
global _start

_start:
    ; Check if we have an argument
    lea rsi, [rel EXEC_ARG_ADDR]
    
    ; Find string length
    xor rcx, rcx
    
find_len:
    cmp byte [rsi + rcx], 0
    je check_empty
    inc rcx
    cmp rcx, EXEC_ARG_MAX
    jae check_empty
    jmp find_len

check_empty:
    ; If empty, just print newline
    test rcx, rcx
    jz print_newline
    
    ; Write the argument
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    ; rsi already points to the string
    mov rdx, rcx
    syscall
    
print_newline:
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

section .rodata
newline: db 0x0D, 0x0A
newline_len equ $ - newline
