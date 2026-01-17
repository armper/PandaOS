; cowtest.asm - COW fork test
; Tests copy-on-write functionality:
; 1. Allocate a page with brk
; 2. Write "X" to it
; 3. Fork
; 4. Child writes "C" to same address and prints it
; 5. Parent prints its value (should still be "X")
; 6. Both exit with status 0

bits 64

; Syscall numbers
SYS_WRITE equ 1
SYS_BRK   equ 12
SYS_FORK  equ 57
SYS_EXIT  equ 60

section .data
    parent_msg: db 'parent: '
    parent_msg_len: equ $ - parent_msg
    child_msg: db 'child: '
    child_msg_len: equ $ - child_msg
    newline: db 10
    success_msg: db 'TEST PASS cowtest', 10
    success_len: equ $ - success_msg
    
section .bss
    heap_ptr: resq 1

section .text
global _start

_start:
    ; Get current brk (heap end)
    mov rax, SYS_BRK
    xor rdi, rdi        ; addr=0 returns current brk
    syscall
    mov [heap_ptr], rax
    
    ; Allocate one page (4096 bytes) via brk
    add rax, 4096
    mov rdi, rax
    mov rax, SYS_BRK
    syscall
    
    ; Write 'X' to the allocated page
    mov rax, [heap_ptr]
    mov byte [rax], 'X'
    
    ; Fork
    mov rax, SYS_FORK
    syscall
    
    test rax, rax
    jz .child
    
.parent:
    ; Parent: print "parent: " and the value (should be 'X')
    mov rax, SYS_WRITE
    mov rdi, 1
    lea rsi, [rel parent_msg]
    mov rdx, parent_msg_len
    syscall
    
    ; Print the character
    mov rax, [heap_ptr]
    mov rax, SYS_WRITE
    mov rdi, 1
    mov rsi, [heap_ptr]
    mov rdx, 1
    syscall
    
    ; Print newline
    mov rax, SYS_WRITE
    mov rdi, 1
    lea rsi, [rel newline]
    mov rdx, 1
    syscall
    
    ; Exit with status 0
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall
    
.child:
    ; Child: write 'C' to the same address
    mov rax, [heap_ptr]
    mov byte [rax], 'C'
    
    ; Print "child: " and the value (should be 'C')
    mov rax, SYS_WRITE
    mov rdi, 1
    lea rsi, [rel child_msg]
    mov rdx, child_msg_len
    syscall
    
    ; Print the character
    mov rax, [heap_ptr]
    mov rax, SYS_WRITE
    mov rdi, 1
    mov rsi, [heap_ptr]
    mov rdx, 1
    syscall
    
    ; Print newline
    mov rax, SYS_WRITE
    mov rdi, 1
    lea rsi, [rel newline]
    mov rdx, 1
    syscall
    
    ; Print success message
    mov rax, SYS_WRITE
    mov rdi, 1
    lea rsi, [rel success_msg]
    mov rdx, success_len
    syscall
    
    ; Exit with status 0
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall
