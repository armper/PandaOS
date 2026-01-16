; Minimal ls for PandaOS
; Lists directory entries using getdents64 syscall
; Supports -l flag for long format output
;
; Usage:
;   ls       - simple list with / suffix for directories
;   ls -l    - long format: drwxr-xr-x  size  name

BITS 64

%define SYS_WRITE 1
%define SYS_OPEN 2
%define SYS_CLOSE 3
%define SYS_STAT 4
%define SYS_EXIT 60
%define SYS_GETDENTS64 217

%define STDOUT 1

%define BUF_SIZE 1024
%define STAT_BUF_SIZE 32        ; Extended stat structure

; Stat structure offsets
%define STAT_MODE 0              ; u16 at offset 0
%define STAT_NLINK 4             ; u32 at offset 4
%define STAT_UID 8               ; u32 at offset 8
%define STAT_GID 12              ; u32 at offset 12
%define STAT_SIZE 16             ; u64 at offset 16
%define STAT_INO 24              ; u64 at offset 24

; Mode bits
%define S_IFMT   0o170000        ; Type mask
%define S_IFDIR  0o040000        ; Directory
%define S_IFREG  0o100000        ; Regular file

; EXEC_ARG_ADDR is where the kernel places the argument string
%define EXEC_ARG_ADDR 0x7FFFFFFFC000

section .text
global _start

_start:
    ; Check for -l flag in argument
    mov rsi, EXEC_ARG_ADDR
    xor rcx, rcx
    
.check_arg:
    mov al, [rsi + rcx]
    test al, al
    jz .no_arg                  ; End of string
    cmp al, '-'
    jne .no_arg
    inc rcx
    mov al, [rsi + rcx]
    cmp al, 'l'
    jne .no_arg
    
    ; Found -l flag
    mov byte [rel long_format], 1
    
.no_arg:
    ; Open root directory "/"
    mov rax, SYS_OPEN
    lea rdi, [rel root_path]
    xor rsi, rsi
    xor rdx, rdx
    syscall
    test rax, rax
    js open_failed
    mov r12, rax            ; r12 = directory fd

read_entries:
    ; Call getdents64
    mov rax, SYS_GETDENTS64
    mov rdi, r12            ; directory fd
    lea rsi, [rel buf]      ; buffer
    mov rdx, BUF_SIZE       ; buffer size
    syscall
    test rax, rax
    js read_failed
    cmp rax, 0
    je done                 ; EOF
    
    mov r13, rax            ; r13 = bytes read
    xor r14, r14            ; r14 = current offset in buffer

process_entry:
    cmp r14, r13
    jge read_entries        ; processed all entries, read more
    
    ; Get pointer to current entry
    lea rbx, [rel buf]
    add rbx, r14
    
    ; Get d_reclen (at offset 16)
    movzx r15, word [rbx + 16]  ; r15 = record length
    
    ; Get d_name (starts at offset 19)
    lea rsi, [rbx + 19]
    
    ; Save the name pointer and record length
    push r15
    push rsi
    
    ; Build full path to stat: "/" + name
    lea rdi, [rel path_buf]
    mov byte [rdi], '/'
    inc rdi
    
.copy_name:
    lodsb
    stosb
    test al, al
    jnz .copy_name
    
    ; Call stat on the full path
    mov rax, SYS_STAT
    lea rdi, [rel path_buf]
    lea rsi, [rel stat_buf]
    syscall
    
    ; Restore name pointer
    pop rsi
    
    ; Check if we're in long format mode
    cmp byte [rel long_format], 1
    je .print_long
    
    ; Simple format: name with optional / suffix
    call print_name
    
    ; Check mode to see if it's a directory
    movzx rax, word [rel stat_buf + STAT_MODE]
    and rax, S_IFMT
    cmp rax, S_IFDIR
    jne .not_dir_simple
    
    ; Print "/" for directories
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel dir_suffix]
    mov rdx, 1
    syscall
    
.not_dir_simple:
    ; Print newline
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, 2
    syscall
    jmp .next_entry
    
.print_long:
    ; Long format: drwxr-xr-x  size  name
    
    ; Get mode
    movzx rax, word [rel stat_buf + STAT_MODE]
    mov [rel current_mode], rax
    
    ; Print file type character
    mov rax, [rel current_mode]
    and rax, S_IFMT
    cmp rax, S_IFDIR
    je .type_dir
    
    ; Regular file
    mov al, '-'
    jmp .type_done
    
.type_dir:
    mov al, 'd'
    
.type_done:
    mov [rel mode_string], al
    
    ; Convert mode bits to rwxr-xr-x string
    mov rax, [rel current_mode]
    lea rdi, [rel mode_string]
    inc rdi                     ; Skip type character
    
    ; User permissions (bits 8-6)
    bt rax, 8                   ; User read
    jc .u_r
    mov byte [rdi], '-'
    jmp .u_r_done
.u_r:
    mov byte [rdi], 'r'
.u_r_done:
    inc rdi
    
    bt rax, 7                   ; User write
    jc .u_w
    mov byte [rdi], '-'
    jmp .u_w_done
.u_w:
    mov byte [rdi], 'w'
.u_w_done:
    inc rdi
    
    bt rax, 6                   ; User execute
    jc .u_x
    mov byte [rdi], '-'
    jmp .u_x_done
.u_x:
    mov byte [rdi], 'x'
.u_x_done:
    inc rdi
    
    ; Group permissions (bits 5-3)
    bt rax, 5                   ; Group read
    jc .g_r
    mov byte [rdi], '-'
    jmp .g_r_done
.g_r:
    mov byte [rdi], 'r'
.g_r_done:
    inc rdi
    
    bt rax, 4                   ; Group write
    jc .g_w
    mov byte [rdi], '-'
    jmp .g_w_done
.g_w:
    mov byte [rdi], 'w'
.g_w_done:
    inc rdi
    
    bt rax, 3                   ; Group execute
    jc .g_x
    mov byte [rdi], '-'
    jmp .g_x_done
.g_x:
    mov byte [rdi], 'x'
.g_x_done:
    inc rdi
    
    ; Other permissions (bits 2-0)
    bt rax, 2                   ; Other read
    jc .o_r
    mov byte [rdi], '-'
    jmp .o_r_done
.o_r:
    mov byte [rdi], 'r'
.o_r_done:
    inc rdi
    
    bt rax, 1                   ; Other write
    jc .o_w
    mov byte [rdi], '-'
    jmp .o_w_done
.o_w:
    mov byte [rdi], 'w'
.o_w_done:
    inc rdi
    
    bt rax, 0                   ; Other execute
    jc .o_x
    mov byte [rdi], '-'
    jmp .o_x_done
.o_x:
    mov byte [rdi], 'x'
.o_x_done:
    
    ; Print mode string (10 characters)
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel mode_string]
    mov rdx, 10
    syscall
    
    ; Print two spaces
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel spaces]
    mov rdx, 2
    syscall
    
    ; Print size
    mov rax, [rel stat_buf + STAT_SIZE]
    call print_number
    
    ; Print two spaces
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel spaces]
    mov rdx, 2
    syscall
    
    ; Print name
    call print_name
    
    ; Print newline
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, 2
    syscall
    
.next_entry:
    ; Restore record length and move to next entry
    pop r15
    add r14, r15
    jmp process_entry

done:
    mov rax, SYS_CLOSE
    mov rdi, r12
    syscall
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

open_failed:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel open_err]
    mov rdx, open_err_len
    syscall
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

read_failed:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel read_err]
    mov rdx, read_err_len
    syscall
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

; print_name: Print null-terminated string
; Input: rsi = pointer to string
print_name:
    push rbx
    push rcx
    mov rbx, rsi
    
    ; Find string length
    xor rcx, rcx
.count:
    cmp byte [rbx + rcx], 0
    je .print
    inc rcx
    cmp rcx, 256
    jl .count
    
.print:
    test rcx, rcx
    jz .done
    
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    mov rsi, rbx
    mov rdx, rcx
    syscall
    
.done:
    pop rcx
    pop rbx
    ret

; print_number: Print decimal number
; Input: rax = number to print
print_number:
    push rbx
    push rcx
    push rdx
    push rdi
    
    ; Handle zero specially
    test rax, rax
    jnz .not_zero
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel zero_char]
    mov rdx, 1
    syscall
    jmp .done
    
.not_zero:
    ; Convert number to string (reverse order)
    lea rdi, [rel num_buf]
    add rdi, 19             ; Point to end of buffer
    mov byte [rdi], 0       ; Null terminate
    dec rdi
    
    mov rbx, 10
    mov rcx, rax            ; Save number
    
.convert_loop:
    xor rdx, rdx
    div rbx                 ; Divide by 10
    add dl, '0'             ; Convert remainder to ASCII
    mov [rdi], dl
    dec rdi
    test rax, rax
    jnz .convert_loop
    
    ; Now rdi points to one before the first digit
    inc rdi
    
    ; Calculate length
    lea rsi, [rel num_buf]
    add rsi, 19
    sub rsi, rdi
    mov rdx, rsi            ; Length in rdx
    
    ; Print it
    mov rax, SYS_WRITE
    mov rsi, rdi            ; String pointer
    mov rdi, STDOUT
    syscall
    
.done:
    pop rdi
    pop rdx
    pop rcx
    pop rbx
    ret

section .data
long_format: db 0
current_mode: dq 0

section .rodata
root_path: db "/", 0
open_err: db "ls: open failed", 0x0D, 0x0A
open_err_len equ $ - open_err
read_err: db "ls: getdents64 failed", 0x0D, 0x0A
read_err_len equ $ - read_err
newline: db 0x0D, 0x0A
dir_suffix: db "/"
spaces: db "  "
zero_char: db "0"

section .bss
buf: resb BUF_SIZE
stat_buf: resb STAT_BUF_SIZE
path_buf: resb 256
mode_string: resb 11        ; drwxr-xr-x + null
num_buf: resb 20
