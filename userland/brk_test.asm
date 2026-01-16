; brk_test.asm - Test program break (heap allocation)
; Tests brk() syscall by:
; 1. Getting current break
; 2. Growing heap by 8KB
; 3. Writing test pattern to new heap memory
; 4. Reading back and verifying
; 5. Shrinking heap back
; 6. Reporting success

bits 64
default rel

global _start

section .data
    msg_start: db "brk_test: starting", 10
    msg_start_len: equ $ - msg_start
    
    msg_get_brk: db "brk_test: getting current break", 10
    msg_get_brk_len: equ $ - msg_get_brk
    
    msg_grow: db "brk_test: growing heap by 8KB", 10
    msg_grow_len: equ $ - msg_grow
    
    msg_write: db "brk_test: writing test pattern", 10
    msg_write_len: equ $ - msg_write
    
    msg_verify: db "brk_test: verifying data", 10
    msg_verify_len: equ $ - msg_verify
    
    msg_shrink: db "brk_test: shrinking heap", 10
    msg_shrink_len: equ $ - msg_shrink
    
    msg_pass: db "TEST PASS brk_smoke", 10
    msg_pass_len: equ $ - msg_pass
    
    msg_fail: db "TEST FAIL brk_smoke", 10
    msg_fail_len: equ $ - msg_fail

section .text
_start:
    ; Print start message
    mov rax, 1              ; sys_write
    mov rdi, 1              ; stdout
    lea rsi, [msg_start]
    mov rdx, msg_start_len
    syscall
    
    ; Get current break (brk(0))
    lea rsi, [msg_get_brk]
    mov rdx, msg_get_brk_len
    call print_msg
    
    mov rax, 12             ; sys_brk
    xor rdi, rdi            ; addr = 0 (query current)
    syscall
    
    cmp rax, 0
    jle .fail               ; Failed to get break
    mov r12, rax            ; Save current break in r12
    
    ; Grow heap by 8KB (2 pages)
    lea rsi, [msg_grow]
    mov rdx, msg_grow_len
    call print_msg
    
    mov rdi, r12
    add rdi, 8192           ; Request 8KB more
    mov rax, 12             ; sys_brk
    syscall
    
    cmp rax, 0
    jle .fail               ; Failed to grow heap
    mov r13, rax            ; Save new break in r13
    
    ; Write test pattern to heap
    lea rsi, [msg_write]
    mov rdx, msg_write_len
    call print_msg
    
    mov rdi, r12            ; Start of new heap area
    mov rcx, 1024           ; Write 1024 values
.write_loop:
    mov rax, rcx
    mov [rdi], rax          ; Write counter value
    add rdi, 8
    loop .write_loop
    
    ; Verify data
    lea rsi, [msg_verify]
    mov rdx, msg_verify_len
    call print_msg
    
    mov rdi, r12            ; Start of heap area
    mov rcx, 1024           ; Check 1024 values
.verify_loop:
    mov rax, rcx
    cmp [rdi], rax
    jne .fail               ; Data mismatch
    add rdi, 8
    loop .verify_loop
    
    ; Shrink heap back
    lea rsi, [msg_shrink]
    mov rdx, msg_shrink_len
    call print_msg
    
    mov rdi, r12            ; Restore original break
    mov rax, 12             ; sys_brk
    syscall
    
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
