; Simple hello world program for PandaOS
; Uses syscalls: write(1, msg, len) and exit(0)

BITS 64

section .text
global _start

_start:
    ; write(1, message, message_len)
    mov rax, 1              ; syscall number for write
    mov rdi, 1              ; fd = stdout
    lea rsi, [rel message]  ; buffer
    mov rdx, message_len    ; count
    syscall                 ; syscall instruction

    ; exit(0)
    mov rax, 60             ; syscall number for exit
    xor rdi, rdi            ; status = 0
    syscall                 ; syscall instruction

section .rodata
message:
    db "hello from user", 0x0A
message_len equ $ - message
