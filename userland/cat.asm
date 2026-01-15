; Minimal cat for PandaOS
; Reads a path from a fixed address and writes file contents to stdout

BITS 64

%define SYS_READ 0
%define SYS_WRITE 1
%define SYS_OPEN 2
%define SYS_CLOSE 3
%define SYS_EXIT 60

%define STDOUT 1

%define ARG_ADDR 0x7FFFFFFFC000
%define BUF_SIZE 256

section .text
global _start

_start:
    mov rax, SYS_OPEN
    mov rdi, ARG_ADDR
    xor rsi, rsi
    xor rdx, rdx
    syscall
    test rax, rax
    js open_failed
    mov r12, rax

read_loop:
    mov rax, SYS_READ
    mov rdi, r12
    lea rsi, [rel buf]
    mov rdx, BUF_SIZE
    syscall
    test rax, rax
    js read_failed
    cmp rax, 0
    je done

    mov rbx, rax
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel buf]
    mov rdx, rbx
    syscall
    jmp read_loop

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

section .rodata
open_err: db "cat: open failed", 0x0D, 0x0A
open_err_len equ $ - open_err
read_err: db "cat: read failed", 0x0D, 0x0A
read_err_len equ $ - read_err

section .bss
buf: resb BUF_SIZE
