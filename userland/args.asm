; args - print argc and argv
; Usage: args [arguments...]
; Prints the number of arguments and each argument on a separate line

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
    ; Then comes envp...

    ; Get argc from stack
    mov rbx, [rsp]              ; rbx = argc
    
    ; Print "argc: "
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel msg_argc]
    mov rdx, msg_argc_len
    syscall
    
    ; Print argc value as decimal
    mov rax, rbx
    call print_number
    
    ; Print newline
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, 1
    syscall
    
    ; Print each argument
    xor r12, r12                ; r12 = current arg index
    
.arg_loop:
    cmp r12, rbx                ; if (i >= argc) break
    jge .done
    
    ; Print "argv["
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel msg_argv_start]
    mov rdx, msg_argv_start_len
    syscall
    
    ; Print index
    mov rax, r12
    call print_number
    
    ; Print "]: "
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel msg_argv_end]
    mov rdx, msg_argv_end_len
    syscall
    
    ; Get argv[i] pointer
    lea rax, [rsp + 8]          ; rax = &argv[0]
    mov rcx, [rax + r12*8]      ; rcx = argv[i]
    
    ; Print the argument string
    mov rdi, rcx
    call print_string
    
    ; Print newline
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, 1
    syscall
    
    inc r12
    jmp .arg_loop
    
.done:
    ; Exit successfully
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

; print_number: print number in rax as decimal
print_number:
    push rbx
    push rcx
    push rdx
    push rsi
    
    ; Handle zero case
    test rax, rax
    jnz .nonzero
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel digit_zero]
    mov rdx, 1
    syscall
    jmp .pn_done
    
.nonzero:
    ; Convert to decimal string (reversed)
    lea rsi, [rel num_buf + 19] ; Point to end of buffer
    mov byte [rsi], 0
    dec rsi
    mov rbx, 10
    mov rcx, rax
    
.pn_loop:
    test rcx, rcx
    jz .pn_print
    xor rdx, rdx
    mov rax, rcx
    div rbx                     ; rax = quot, rdx = rem
    add dl, '0'
    mov [rsi], dl
    dec rsi
    mov rcx, rax
    jmp .pn_loop
    
.pn_print:
    inc rsi                     ; Adjust to first digit
    lea rax, [rel num_buf + 20]
    sub rax, rsi                ; Length
    mov rdx, rax
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    syscall
    
.pn_done:
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    ret

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
msg_argc: db "argc: "
msg_argc_len equ $ - msg_argc

msg_argv_start: db "argv["
msg_argv_start_len equ $ - msg_argv_start

msg_argv_end: db "]: "
msg_argv_end_len equ $ - msg_argv_end

newline: db 10
digit_zero: db '0'

section .bss
num_buf: resb 20
