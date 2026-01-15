; Init process for PandaOS
; Executes /bin/sh via execve and exits on failure

BITS 64

section .text
global _start

_start:
    ; execve("/bin/sh", NULL, NULL)
    mov rax, 59             ; syscall number for execve
    lea rdi, [rel path]     ; filename
    xor rsi, rsi            ; argv = NULL
    xor rdx, rdx            ; envp = NULL
    syscall

    ; If exec fails, exit(1)
    mov rax, 60             ; syscall number for exit
    mov rdi, 1              ; status = 1
    syscall

section .rodata
path:
    db "/bin/sh", 0
