; Dummy shell for PandaOS
; Prints a prompt and exits

BITS 64

section .text
global _start

_start:
    ; write(1, prompt, prompt_len)
    mov rax, 1              ; syscall number for write
    mov rdi, 1              ; fd = stdout
    lea rsi, [rel prompt]   ; buffer
    mov rdx, prompt_len     ; count
    syscall

    ; exit(0)
    mov rax, 60             ; syscall number for exit
    xor rdi, rdi            ; status = 0
    syscall

section .rodata
prompt:
    db "panda> ", 0x0A
prompt_len equ $ - prompt
