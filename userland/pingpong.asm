; pingpong.asm - Two processes alternating without yielding
; This program forks and both parent and child print messages
; in an infinite loop without ever calling yield().
; Preemptive multitasking should cause them to alternate.

bits 64

; Syscall numbers
SYS_WRITE equ 1
SYS_FORK  equ 57
SYS_EXIT  equ 60

section .data
    ping_msg: db 'ping '
    ping_len: equ $ - ping_msg
    pong_msg: db 'pong '
    pong_len: equ $ - pong_msg

section .text
global _start

_start:
    ; Fork to create two processes
    mov rax, SYS_FORK
    syscall
    
    ; Check if we're parent (rax > 0) or child (rax == 0)
    test rax, rax
    jz .child
    
.parent:
    ; Parent process prints "ping" repeatedly
.parent_loop:
    mov rax, SYS_WRITE
    mov rdi, 1              ; stdout
    lea rsi, [rel ping_msg]
    mov rdx, ping_len
    syscall
    jmp .parent_loop
    
.child:
    ; Child process prints "pong" repeatedly
.child_loop:
    mov rax, SYS_WRITE
    mov rdi, 1              ; stdout
    lea rsi, [rel pong_msg]
    mov rdx, pong_len
    syscall
    jmp .child_loop
    
    ; Exit (never reached)
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall
