; printenv - print environment variables
; Usage: printenv
; Prints each environment variable on a separate line

BITS 64

%define SYS_WRITE 1
%define SYS_EXIT 60
%define STDOUT 1

section .text
global _start

_start:
    ; Stack layout at entry (Linux ABI):
    ; [rsp]     = argc
    ; [rsp+8]   = argv[0]
    ; [rsp+16]  = argv[1]
    ; ...
    ; [rsp+8*(argc+1)] = NULL
    ; [rsp+8*(argc+2)] = envp[0]
    ; [rsp+8*(argc+3)] = envp[1]
    ; ...
    ; [rsp+8*(argc+n)] = NULL

    ; Get argc from stack
    mov rbx, [rsp]              ; rbx = argc
    
    ; Calculate envp address: &argv[argc+1]
    ; envp = rsp + 8 + (argc + 1) * 8
    lea r12, [rsp + 8]          ; r12 = &argv[0]
    lea r12, [r12 + rbx*8 + 8]  ; r12 = &envp[0] (skip argv array + NULL)
    
    ; Check if we have any environment
    mov rax, [r12]
    test rax, rax
    jnz .has_env
    
    ; No environment - print message
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel msg_no_env]
    mov rdx, msg_no_env_len
    syscall
    jmp .done
    
.has_env:
    ; Print each environment variable
    xor r13, r13                ; r13 = env index
    
.env_loop:
    mov rcx, [r12 + r13*8]      ; rcx = envp[i]
    test rcx, rcx               ; if (envp[i] == NULL) break
    jz .done
    
    ; Print the environment string
    mov rdi, rcx
    call print_string
    
    ; Print newline
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, 1
    syscall
    
    inc r13
    jmp .env_loop
    
.done:
    ; Exit successfully
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

; print_string: print null-terminated string at rdi
print_string:
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    
    ; Find string length
    mov rsi, rdi
    xor rcx, rcx
.ps_len:
    mov al, [rsi + rcx]
    test al, al
    jz .ps_print
    inc rcx
    cmp rcx, 1024               ; Max length
    jl .ps_len
    
.ps_print:
    mov rdx, rcx                ; Length
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    syscall
    
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    ret

section .data
msg_no_env: db "(no environment)", 10
msg_no_env_len equ $ - msg_no_env

newline: db 10
