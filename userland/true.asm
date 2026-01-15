; /bin/true - exits with status 0
; Minimal program that just exits successfully

BITS 64

%define SYS_EXIT 60

section .text
global _start

_start:
    ; exit(0)
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall
