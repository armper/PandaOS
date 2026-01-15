; Minimal shell for PandaOS
; REPL over serial using read/write syscalls

BITS 64

%define SYS_READ 0
%define SYS_WRITE 1
%define SYS_EXIT 60

%define STDIN 0
%define STDOUT 1

%define BUF_SIZE 128

section .text
global _start

_start:
    lea r13, [rel line_buf]

main_loop:
    ; write prompt
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel prompt]
    mov rdx, prompt_len
    syscall

    xor r12, r12

read_loop:
    ; read one byte
    mov rax, SYS_READ
    mov rdi, STDIN
    lea rsi, [rel input_char]
    mov rdx, 1
    syscall

    mov al, [rel input_char]
    cmp al, 0x0D
    je line_done
    cmp al, 0x0A
    je line_done
    cmp al, 0x08
    je handle_backspace
    cmp al, 0x7F
    je handle_backspace

    cmp al, 0x20
    jb read_loop
    cmp al, 0x7E
    ja read_loop

    cmp r12, BUF_SIZE - 1
    jae read_loop

    mov [r13 + r12], al
    inc r12

    ; echo character
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel input_char]
    mov rdx, 1
    syscall
    jmp read_loop

handle_backspace:
    cmp r12, 0
    je read_loop
    dec r12
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel bs_seq]
    mov rdx, bs_seq_len
    syscall
    jmp read_loop

line_done:
    mov byte [r13 + r12], 0

    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, newline_len
    syscall

    cmp r12, 0
    je main_loop

    ; help
    cmp r12, 4
    jne check_exit
    mov al, [r13]
    cmp al, 'h'
    jne check_exit
    mov al, [r13 + 1]
    cmp al, 'e'
    jne check_exit
    mov al, [r13 + 2]
    cmp al, 'l'
    jne check_exit
    mov al, [r13 + 3]
    cmp al, 'p'
    jne check_exit
    jmp cmd_help

check_exit:
    cmp r12, 4
    jne check_echo
    mov al, [r13]
    cmp al, 'e'
    jne check_echo
    mov al, [r13 + 1]
    cmp al, 'x'
    jne check_echo
    mov al, [r13 + 2]
    cmp al, 'i'
    jne check_echo
    mov al, [r13 + 3]
    cmp al, 't'
    jne check_echo
    jmp cmd_exit

check_echo:
    cmp r12, 4
    jl cmd_unknown
    mov al, [r13]
    cmp al, 'e'
    jne cmd_unknown
    mov al, [r13 + 1]
    cmp al, 'c'
    jne cmd_unknown
    mov al, [r13 + 2]
    cmp al, 'h'
    jne cmd_unknown
    mov al, [r13 + 3]
    cmp al, 'o'
    jne cmd_unknown

    cmp r12, 4
    je cmd_echo_empty
    mov al, [r13 + 4]
    cmp al, ' '
    jne cmd_unknown

    lea rsi, [r13 + 5]
    mov rdx, r12
    sub rdx, 5
    jmp cmd_echo_arg

cmd_help:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel help_text]
    mov rdx, help_len
    syscall
    jmp main_loop

cmd_echo_empty:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, newline_len
    syscall
    jmp main_loop

cmd_echo_arg:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    syscall

    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, newline_len
    syscall
    jmp main_loop

cmd_exit:
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

cmd_unknown:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel unknown_text]
    mov rdx, unknown_len
    syscall
    jmp main_loop

section .rodata
prompt: db "panda> "
prompt_len equ $ - prompt
help_text: db "commands: help, echo, exit", 0x0D, 0x0A
help_len equ $ - help_text
unknown_text: db "command not found", 0x0D, 0x0A
unknown_len equ $ - unknown_text
bs_seq: db 0x08, ' ', 0x08
bs_seq_len equ $ - bs_seq
newline: db 0x0D, 0x0A
newline_len equ $ - newline

section .bss
line_buf: resb BUF_SIZE
input_char: resb 1
