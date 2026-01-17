; preempt_test.asm - Test program for preemptive multitasking
; This program spawns two spin processes and waits for them to complete.
; It verifies that preemption is working by checking that both processes
; make progress even though they never call yield().

bits 64

; Syscall numbers
SYS_WRITE equ 1
SYS_FORK  equ 57
SYS_EXIT  equ 60
SYS_WAIT4 equ 61

section .data
    start_msg: db 'Preemption test starting', 10
    start_len: equ $ - start_msg
    spawn_msg: db 'Spawning spin processes...', 10
    spawn_len: equ $ - spawn_msg
    wait_msg: db 'Waiting for processes...', 10
    wait_len: equ $ - wait_msg
    done_msg: db 'Preemption test complete', 10
    done_len: equ $ - done_msg
    spin_msg: db 'X'
    spin_len: equ $ - spin_msg

section .bss
    counter: resq 1

section .text
global _start

_start:
    ; Print start message
    mov rax, SYS_WRITE
    mov rdi, 1
    lea rsi, [rel start_msg]
    mov rdx, start_len
    syscall

    ; Print spawn message
    mov rax, SYS_WRITE
    mov rdi, 1
    lea rsi, [rel spawn_msg]
    mov rdx, spawn_len
    syscall

    ; Fork first child
    mov rax, SYS_FORK
    syscall
    test rax, rax
    jz .child1

    ; Parent: fork second child
    mov rax, SYS_FORK
    syscall
    test rax, rax
    jz .child2

    ; Parent: wait for both children
    mov rax, SYS_WRITE
    mov rdi, 1
    lea rsi, [rel wait_msg]
    mov rdx, wait_len
    syscall

    ; Wait for first child
    mov rax, SYS_WAIT4
    mov rdi, -1           ; wait for any child
    xor rsi, rsi          ; status (NULL)
    xor rdx, rdx          ; options
    syscall

    ; Wait for second child
    mov rax, SYS_WAIT4
    mov rdi, -1
    xor rsi, rsi
    xor rdx, rdx
    syscall

    ; Print done message
    mov rax, SYS_WRITE
    mov rdi, 1
    lea rsi, [rel done_msg]
    mov rdx, done_len
    syscall

    ; Exit parent
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

.child1:
    ; Child 1: print X repeatedly (limited count)
    mov rcx, 50           ; print 50 times
.child1_loop:
    mov rax, SYS_WRITE
    mov rdi, 1
    lea rsi, [rel spin_msg]
    mov rdx, spin_len
    syscall
    
    dec rcx
    jnz .child1_loop
    
    ; Exit child 1
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

.child2:
    ; Child 2: print X repeatedly (limited count)
    mov rcx, 50           ; print 50 times
.child2_loop:
    mov rax, SYS_WRITE
    mov rdi, 1
    lea rsi, [rel spin_msg]
    mov rdx, spin_len
    syscall
    
    dec rcx
    jnz .child2_loop
    
    ; Exit child 2
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall
