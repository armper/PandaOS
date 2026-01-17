; spin.asm - Infinite loop that never yields
; This program tests preemptive multitasking by running an infinite loop
; that only does syscalls (write) without ever calling yield().
; Without preemption, this would monopolize the CPU.

bits 64

; Syscall numbers
SYS_WRITE equ 1
SYS_EXIT  equ 60

section .data
    msg: db 'A'
    msg_len: equ $ - msg

section .text
global _start

_start:
    ; Infinite loop that prints 'A' repeatedly
    ; This should be preempted by the timer interrupt
.loop:
    ; Write 'A' to stdout
    mov rax, SYS_WRITE      ; syscall number: write
    mov rdi, 1              ; fd: stdout
    lea rsi, [rel msg]      ; buffer
    mov rdx, msg_len        ; count
    syscall

    ; Loop forever - no yield!
    jmp .loop

    ; Exit (never reached, but good practice)
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall
