; chmod for PandaOS
; Usage: chmod <mode> <path>
; Example: chmod 755 /mnt/bin/ls
; Reads argument string from fixed address (format: "755 /mnt/bin/ls")

BITS 64

%define SYS_WRITE 1
%define SYS_CHMOD 90
%define SYS_EXIT 60

%define STDOUT 1
%define ARG_ADDR 0x7FFFFFFFC000
%define MAX_ARG_LEN 128

section .text
global _start

_start:
    ; Parse arguments from ARG_ADDR (format: "755 /mnt/bin/ls")
    mov rsi, ARG_ADDR
    
    ; Parse octal mode (3 digits)
    xor rax, rax            ; mode accumulator
    xor rcx, rcx            ; digit counter
    
parse_mode:
    movzx rdx, byte [rsi]
    
    ; Check for space (end of mode)
    cmp dl, ' '
    je mode_done
    
    ; Check for null terminator
    test dl, dl
    jz usage_error
    
    ; Check if digit (0-7 for octal)
    cmp dl, '0'
    jb usage_error
    cmp dl, '7'
    ja usage_error
    
    ; Convert digit and add to mode (mode = mode * 8 + digit)
    sub dl, '0'
    shl rax, 3              ; mode *= 8
    add rax, rdx            ; mode += digit
    
    inc rsi
    inc rcx
    cmp rcx, 4              ; max 4 octal digits (0777)
    jb parse_mode
    
usage_error:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel usage_msg]
    mov rdx, usage_msg_len
    syscall
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

mode_done:
    ; Check that we parsed at least one digit
    test rcx, rcx
    jz usage_error
    
    ; rax now contains the mode
    mov r12, rax            ; save mode in r12
    
    ; Skip space
    inc rsi
    
    ; Check for path (rsi now points to path)
    movzx rdx, byte [rsi]
    test rdx, rdx
    jz usage_error
    
    ; Call chmod syscall
    mov rax, SYS_CHMOD
    mov rdi, rsi            ; path pointer
    mov rsi, r12            ; mode
    syscall
    
    ; Check result
    test rax, rax
    js chmod_failed
    
    ; Success
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

chmod_failed:
    ; Print error message
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel error_msg]
    mov rdx, error_msg_len
    syscall
    
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

section .rodata
usage_msg: db "Usage: chmod <mode> <path>", 0x0D, 0x0A, "Example: chmod 755 /mnt/bin/ls", 0x0D, 0x0A
usage_msg_len equ $ - usage_msg

error_msg: db "chmod: failed", 0x0D, 0x0A
error_msg_len equ $ - error_msg
