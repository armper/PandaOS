; sleepy - Simple program that loops and yields
; Used for testing job control (Ctrl+Z / fg)

BITS 64

%define SYS_WRITE 1
%define SYS_YIELD 24
%define SYS_EXIT 60

%define STDOUT 1

section .text
global _start

_start:
    ; Loop forever, printing "tick" and yielding
loop:
    ; Print "tick\n"
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel tick_msg]
    mov rdx, tick_msg_len
    syscall
    
    ; Yield CPU to allow other processes to run and signals to be delivered
    mov rax, SYS_YIELD
    syscall
    
    ; Loop again
    jmp loop

section .rodata
tick_msg: db "tick", 0x0A  ; "tick\n"
tick_msg_len: equ $ - tick_msg
