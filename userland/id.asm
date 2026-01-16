; id for PandaOS
; Displays the current uid and gid
; Usage: id

BITS 64

%define SYS_WRITE 1
%define SYS_GETUID 102
%define SYS_GETGID 104
%define SYS_EXIT 60

%define STDOUT 1

section .data
    uid_label: db "uid=", 0
    uid_label_len: equ $ - uid_label
    gid_label: db " gid=", 0
    gid_label_len: equ $ - gid_label
    newline: db 10
    
section .bss
    uid_buf: resb 12  ; Buffer for uid number
    gid_buf: resb 12  ; Buffer for gid number

section .text
global _start

_start:
    ; Get uid
    mov rax, SYS_GETUID
    syscall
    mov r12, rax        ; Save uid in r12
    
    ; Get gid
    mov rax, SYS_GETGID
    syscall
    mov r13, rax        ; Save gid in r13
    
    ; Print "uid="
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, uid_label
    mov rdx, uid_label_len - 1  ; Exclude null terminator
    syscall
    
    ; Convert uid to string and print
    mov rax, r12
    mov rdi, uid_buf
    call num_to_str
    
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, uid_buf
    mov rdx, rcx        ; Length from num_to_str
    syscall
    
    ; Print " gid="
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, gid_label
    mov rdx, gid_label_len - 1  ; Exclude null terminator
    syscall
    
    ; Convert gid to string and print
    mov rax, r13
    mov rdi, gid_buf
    call num_to_str
    
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, gid_buf
    mov rdx, rcx        ; Length from num_to_str
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
