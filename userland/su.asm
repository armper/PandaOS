; su for PandaOS
; Switch user identity
; Usage: su <uid>
; Reads argument from ARG_ADDR (format: "1000")

BITS 64

%define SYS_WRITE 1
%define SYS_SETUID 105
%define SYS_SETGID 106
%define SYS_EXIT 60

%define STDOUT 1
%define ARG_ADDR 0x7FFFFFFFC000

section .data
    success_msg: db "Switched to uid=", 0
    success_len: equ $ - success_msg
    eperm_msg: db "su: Permission denied", 10
    eperm_len: equ $ - eperm_msg
    usage_msg: db "Usage: su <uid>", 10
    usage_len: equ $ - usage_msg
    newline: db 10

section .bss
    uid_buf: resb 12

section .text
global _start

_start:
    ; Parse uid from ARG_ADDR
    mov rsi, ARG_ADDR
    xor rax, rax        ; uid accumulator
    
parse_uid:
    movzx rdx, byte [rsi]
    
    ; Check for null terminator or newline
    test dl, dl
    jz parse_done
    cmp dl, 10
    je parse_done
    cmp dl, ' '
    je parse_done
    
    ; Check if digit
    cmp dl, '0'
    jb usage_error
    cmp dl, '9'
    ja usage_error
    
    ; Convert digit and add to uid (uid = uid * 10 + digit)
    sub dl, '0'
    imul rax, 10
    add rax, rdx
    
    inc rsi
    jmp parse_uid
    
parse_done:
    ; rax now contains the target uid
    mov r12, rax        ; Save uid
    
    ; Call setgid(uid) - set gid to same value as uid
    mov rax, SYS_SETGID
    mov rdi, r12
    syscall
    
    ; Check for error (negative return value)
    test rax, rax
    js setgid_error
    
    ; Call setuid(uid)
    mov rax, SYS_SETUID
    mov rdi, r12
    syscall
    
    ; Check for error (negative return value)
    test rax, rax
    js setuid_error
    
    ; Success - print message
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, success_msg
    mov rdx, success_len - 1
    syscall
    
    ; Print uid
    mov rax, r12
    mov rdi, uid_buf
    call num_to_str
    
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, uid_buf
    mov rdx, rcx
    syscall
    
    ; Print newline
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, newline
    mov rdx, 1
    syscall
    
    ; exit(0)
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall
    
setuid_error:
setgid_error:
    ; Print permission denied error
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, eperm_msg
    mov rdx, eperm_len
    syscall
    
    ; exit(1)
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall
    
usage_error:
    ; Print usage message
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, usage_msg
    mov rdx, usage_len
    syscall
    
    ; exit(1)
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

; Convert number in rax to decimal string at rdi
; Returns length in rcx
num_to_str:
    push rbx
    push rdx
    push rsi
    
    mov rbx, rdi        ; Save buffer pointer
    mov rcx, 0          ; Digit counter
    mov rsi, 10         ; Divisor
    
    ; Handle zero specially
    test rax, rax
    jnz .convert
    mov byte [rdi], '0'
    inc rdi
    mov rcx, 1
    jmp .done
    
.convert:
    ; Convert digits in reverse
    mov rdi, rbx
    add rdi, 11         ; Point to end of buffer
    
.digit_loop:
    test rax, rax
    jz .reverse
    
    xor rdx, rdx
    div rsi             ; rax = rax / 10, rdx = rax % 10
    add dl, '0'         ; Convert to ASCII
    dec rdi
    mov [rdi], dl
    inc rcx
    jmp .digit_loop
    
.reverse:
    ; Move digits to start of buffer
    mov rsi, rdi        ; Source
    mov rdi, rbx        ; Destination
    push rcx
    rep movsb
    pop rcx
    
.done:
    pop rsi
    pop rdx
    pop rbx
    ret
