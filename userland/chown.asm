; chown for PandaOS
; Change file ownership
; Usage: chown <uid> <gid> <path>
; Reads arguments from ARG_ADDR (format: "1000 1000 /tmp/test.txt")

BITS 64

%define SYS_WRITE 1
%define SYS_CHOWN 92
%define SYS_EXIT 60

%define STDOUT 1
%define ARG_ADDR 0x7FFFFFFFC000

section .data
    success_msg: db "chown: ownership changed", 10
    success_len: equ $ - success_msg
    eperm_msg: db "chown: Permission denied", 10
    eperm_len: equ $ - eperm_msg
    usage_msg: db "Usage: chown <uid> <gid> <path>", 10
    usage_len: equ $ - usage_msg
    enoent_msg: db "chown: No such file", 10
    enoent_len: equ $ - enoent_msg

section .text
global _start

_start:
    ; Parse uid from ARG_ADDR
    mov rsi, ARG_ADDR
    xor rax, rax        ; uid accumulator
    
parse_uid:
    movzx rdx, byte [rsi]
    
    ; Check for space (end of uid)
    cmp dl, ' '
    je uid_done
    
    ; Check for null terminator
    test dl, dl
    jz usage_error
    
    ; Check if digit
    cmp dl, '0'
    jb usage_error
    cmp dl, '9'
    ja usage_error
    
    ; Convert digit and add to uid
    sub dl, '0'
    imul rax, 10
    add rax, rdx
    
    inc rsi
    jmp parse_uid
    
uid_done:
    mov r12, rax        ; Save uid in r12
    inc rsi             ; Skip space
    
    ; Parse gid
    xor rax, rax        ; gid accumulator
    
parse_gid:
    movzx rdx, byte [rsi]
    
    ; Check for space (end of gid)
    cmp dl, ' '
    je gid_done
    
    ; Check for null terminator
    test dl, dl
    jz usage_error
    
    ; Check if digit
    cmp dl, '0'
    jb usage_error
    cmp dl, '9'
    ja usage_error
    
    ; Convert digit and add to gid
    sub dl, '0'
    imul rax, 10
    add rax, rdx
    
    inc rsi
    jmp parse_gid
    
gid_done:
    mov r13, rax        ; Save gid in r13
    inc rsi             ; Skip space
    
    ; rsi now points to path
    ; Find length of path
    mov rdi, rsi
    xor rcx, rcx
    
find_path_len:
    movzx rdx, byte [rdi]
    test dl, dl
    jz path_len_found
    cmp dl, 10          ; Newline
    je path_len_found
    inc rdi
    inc rcx
    jmp find_path_len
    
path_len_found:
    ; Check if we have a path
    test rcx, rcx
    jz usage_error
    
    ; Call chown syscall
    ; chown(path, uid, gid)
    mov rax, SYS_CHOWN
    mov rdi, rsi        ; path pointer
    mov rsi, r12        ; uid
    mov rdx, r13        ; gid
    syscall
    
    ; Check for error
    test rax, rax
    js check_error
    
    ; Success
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, success_msg
    mov rdx, success_len
    syscall
    
    ; exit(0)
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall
    
check_error:
    ; Check if EPERM (errno 1 returned as -1)
    cmp rax, -1
    je eperm_error
    
    ; Check if ENOENT (errno 2 returned as -2)
    cmp rax, -2
    je enoent_error
    
    ; Generic error - treat as EPERM
    jmp eperm_error
    
eperm_error:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, eperm_msg
    mov rdx, eperm_len
    syscall
    
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall
    
enoent_error:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, enoent_msg
    mov rdx, enoent_len
    syscall
    
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall
    
usage_error:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, usage_msg
    mov rdx, usage_len
    syscall
    
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall
