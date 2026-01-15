; Minimal shell for PandaOS
; REPL over serial using read/write syscalls

BITS 64

%define SYS_READ 0
%define SYS_WRITE 1
%define SYS_FORK 57
%define SYS_EXECVE 59
%define SYS_EXIT 60
%define SYS_WAIT4 61

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
    jl check_cat
    mov al, [r13]
    cmp al, 'e'
    jne check_cat
    mov al, [r13 + 1]
    cmp al, 'c'
    jne check_cat
    mov al, [r13 + 2]
    cmp al, 'h'
    jne check_cat
    mov al, [r13 + 3]
    cmp al, 'o'
    jne check_cat

    cmp r12, 4
    je cmd_echo_empty
    mov al, [r13 + 4]
    cmp al, ' '
    jne check_cat

    lea rsi, [r13 + 5]
    mov rdx, r12
    sub rdx, 5
    jmp cmd_echo_arg

check_cat:
    cmp r12, 3
    jl check_true
    mov al, [r13]
    cmp al, 'c'
    jne check_true
    mov al, [r13 + 1]
    cmp al, 'a'
    jne check_true
    mov al, [r13 + 2]
    cmp al, 't'
    jne check_true

    cmp r12, 3
    je cmd_cat_usage
    mov al, [r13 + 3]
    cmp al, ' '
    jne check_true
    cmp r12, 4
    je cmd_cat_usage

    lea rsi, [r13 + 4]
    lea rdi, [rel cat_path]
    jmp cmd_fork_exec

check_true:
    cmp r12, 4
    jl cmd_unknown
    mov al, [r13]
    cmp al, 't'
    jne cmd_unknown
    mov al, [r13 + 1]
    cmp al, 'r'
    jne cmd_unknown
    mov al, [r13 + 2]
    cmp al, 'u'
    jne cmd_unknown
    mov al, [r13 + 3]
    cmp al, 'e'
    jne cmd_unknown
    cmp r12, 4
    jne cmd_unknown

    ; Run /bin/true
    xor rsi, rsi
    lea rdi, [rel true_path]
    jmp cmd_fork_exec

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

cmd_cat_usage:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel cat_usage]
    mov rdx, cat_usage_len
    syscall
    jmp main_loop

cmd_fork_exec:
    ; rdi = program path
    ; rsi = argument (or 0 for none)
    ; Save these across fork
    mov r14, rdi
    mov r15, rsi

    ; fork()
    mov rax, SYS_FORK
    syscall
    test rax, rax
    js fork_failed
    jz child_process

    ; Parent process - wait for child
    mov r12, rax  ; Save child PID
    
parent_wait:
    mov rax, SYS_WAIT4
    mov rdi, r12  ; Wait for specific child
    xor rsi, rsi  ; Don't care about status
    xor rdx, rdx  ; options = 0
    syscall
    test rax, rax
    js parent_wait  ; If error (EAGAIN/EINTR), retry
    
    ; Child exited, continue shell
    jmp main_loop

child_process:
    ; Child process - exec the program
    mov rax, SYS_EXECVE
    mov rdi, r14
    mov rsi, r15
    xor rdx, rdx
    syscall
    
    ; If exec returns, it failed
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel exec_fail]
    mov rdx, exec_fail_len
    syscall
    
    ; Exit child with error
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

fork_failed:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel fork_fail]
    mov rdx, fork_fail_len
    syscall
    jmp main_loop

section .rodata
prompt: db "panda> "
prompt_len equ $ - prompt
help_text: db "commands: help, echo, cat, true, exit", 0x0D, 0x0A
help_len equ $ - help_text
unknown_text: db "command not found", 0x0D, 0x0A
unknown_len equ $ - unknown_text
bs_seq: db 0x08, ' ', 0x08
bs_seq_len equ $ - bs_seq
newline: db 0x0D, 0x0A
newline_len equ $ - newline
cat_usage: db "usage: cat <path>", 0x0D, 0x0A
cat_usage_len equ $ - cat_usage
exec_fail: db "exec failed", 0x0D, 0x0A
exec_fail_len equ $ - exec_fail
fork_fail: db "fork failed", 0x0D, 0x0A
fork_fail_len equ $ - fork_fail
cat_path: db "/bin/cat", 0
true_path: db "/bin/true", 0

section .bss
line_buf: resb BUF_SIZE
input_char: resb 1
