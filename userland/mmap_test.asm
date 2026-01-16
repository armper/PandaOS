; mmap_test.asm - Test anonymous memory mapping
; Tests mmap() syscall by:
; 1. Mapping 8KB of anonymous memory
; 2. Writing test pattern
; 3. Reading back and verifying
; 4. Reporting success

bits 64
default rel

global _start

section .data
    msg_start: db "mmap_test: starting", 10
    msg_start_len: equ $ - msg_start
    
    msg_map: db "mmap_test: mapping 8KB anonymous memory", 10
    msg_map_len: equ $ - msg_map
    
    msg_write: db "mmap_test: writing test pattern", 10
    msg_write_len: equ $ - msg_write
    
    msg_verify: db "mmap_test: verifying data", 10
    msg_verify_len: equ $ - msg_verify
    
    msg_pass: db "TEST PASS mmap_smoke", 10
    msg_pass_len: equ $ - msg_pass
    
    msg_fail: db "TEST FAIL mmap_smoke", 10
    msg_fail_len: equ $ - msg_fail

section .text
_start:
    ; Print start message
    mov rax, 1              ; sys_write
    mov rdi, 1              ; stdout
    lea rsi, [msg_start]
    mov rdx, msg_start_len
    syscall
    
    ; Map 8KB of anonymous memory
    lea rsi, [msg_map]
    mov rdx, msg_map_len
    call print_msg
    
    mov rax, 9              ; sys_mmap
    xor rdi, rdi            ; addr = 0 (kernel chooses)
    mov rsi, 8192           ; length = 8KB
    mov rdx, 3              ; prot = PROT_READ | PROT_WRITE
    mov r10, 0x22           ; flags = MAP_PRIVATE | MAP_ANONYMOUS
    mov r8, -1              ; fd = -1 (anonymous)
    xor r9, r9              ; offset = 0
    syscall
    
    ; Check if mmap succeeded (returns address or negative errno)
    test rax, rax
    js .fail                ; Negative means error
    cmp rax, 0
    je .fail                ; Zero is also invalid
    
    mov r12, rax            ; Save mapped address in r12
    
    ; Write test pattern to mapped memory
    lea rsi, [msg_write]
    mov rdx, msg_write_len
    call print_msg
    
    mov rdi, r12            ; Start of mapped area
    mov rcx, 1024           ; Write 1024 qwords (8KB)
.write_loop:
    mov rax, rcx
    mov [rdi], rax          ; Write counter value
    add rdi, 8
    loop .write_loop
    
    ; Verify data
    lea rsi, [msg_verify]
    mov rdx, msg_verify_len
    call print_msg
    
    mov rdi, r12            ; Start of mapped area
    mov rcx, 1024           ; Check 1024 qwords
.verify_loop:
    mov rax, rcx
    cmp [rdi], rax
    jne .fail               ; Data mismatch
    add rdi, 8
    loop .verify_loop
    
    ; Success!
    lea rsi, [msg_pass]
    mov rdx, msg_pass_len
    call print_msg
    
    mov rax, 60             ; sys_exit
    xor rdi, rdi            ; status = 0
    syscall

.fail:
    lea rsi, [msg_fail]
    mov rdx, msg_fail_len
    call print_msg
    
    mov rax, 60             ; sys_exit
    mov rdi, 1              ; status = 1
    syscall

; Helper function to print message
print_msg:
    push rax
    push rdi
    mov rax, 1              ; sys_write
    mov rdi, 1              ; stdout
    ; rsi and rdx already set by caller
    syscall
    pop rdi
    pop rax
    ret
