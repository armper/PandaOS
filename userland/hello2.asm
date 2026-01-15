; Process 2 for scheduler testing
; Prints message and yields/exits

BITS 64

section .text
global _start

_start:
    ; Loop 5 times
    mov r12, 5              ; Counter

.loop:
    ; write(1, message, message_len)
    mov rax, 1              ; syscall number for write
    mov rdi, 1              ; fd = stdout
    lea rsi, [rel message]  ; buffer
    mov rdx, message_len    ; count
    syscall

    ; Decrement counter
    dec r12
    jz .exit

    ; yield() - syscall 24
    mov rax, 24             ; syscall number for yield
    syscall

    jmp .loop

.exit:
    ; exit(0)
    mov rax, 60             ; syscall number for exit
    xor rdi, rdi            ; status = 0
    syscall

section .rodata
message:
    db "[P2] Process 2 running", 0x0A
message_len equ $ - message
