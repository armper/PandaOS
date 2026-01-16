; vm_test.asm - Comprehensive VM test with brk, mmap, and fork
; Tests:
; 1. Allocate heap with brk
; 2. Allocate mmap region
; 3. Write data to both
; 4. Fork and verify parent/child isolation
; 5. Modify data in child
; 6. Verify parent data unchanged

bits 64
default rel

global _start

section .data
    msg_start: db "vm_test: starting comprehensive VM test", 10
    msg_start_len: equ $ - msg_start
    
    msg_brk: db "vm_test: allocating heap with brk", 10
    msg_brk_len: equ $ - msg_brk
    
    msg_mmap: db "vm_test: allocating mmap region", 10
    msg_mmap_len: equ $ - msg_mmap
    
    msg_write_parent: db "vm_test: writing parent data", 10
    msg_write_parent_len: equ $ - msg_write_parent
    
    msg_fork: db "vm_test: forking", 10
    msg_fork_len: equ $ - msg_fork
    
    msg_child: db "vm_test: in child process", 10
    msg_child_len: equ $ - msg_child
    
    msg_child_modify: db "vm_test: child modifying data", 10
    msg_child_modify_len: equ $ - msg_child_modify
    
    msg_child_exit: db "vm_test: child exiting", 10
    msg_child_exit_len: equ $ - msg_child_exit
    
    msg_parent: db "vm_test: in parent process", 10
    msg_parent_len: equ $ - msg_parent
    
    msg_parent_wait: db "vm_test: parent waiting for child", 10
    msg_parent_wait_len: equ $ - msg_parent_wait
    
    msg_parent_verify: db "vm_test: parent verifying data unchanged", 10
    msg_parent_verify_len: equ $ - msg_parent_verify
    
    msg_pass: db "TEST PASS vm_smoke", 10
    msg_pass_len: equ $ - msg_pass
    
    msg_fail: db "TEST FAIL vm_smoke", 10
    msg_fail_len: equ $ - msg_fail

section .bss
    brk_addr: resq 1        ; Store heap address
    mmap_addr: resq 1       ; Store mmap address
    child_pid: resq 1       ; Store child PID

section .text
_start:
    ; Print start message
    lea rsi, [msg_start]
    mov rdx, msg_start_len
    call print_msg
    
    ; === Get current brk ===
    lea rsi, [msg_brk]
    mov rdx, msg_brk_len
    call print_msg
    
    mov rax, 12             ; sys_brk
    xor rdi, rdi            ; addr = 0 (query current)
    syscall
    
    test rax, rax
    jz .fail                ; Failed to get break
    mov [brk_addr], rax     ; Save current break
    
    ; === Grow heap by 8KB ===
    mov rdi, rax
    add rdi, 8192           ; Request 8KB more
    mov rax, 12             ; sys_brk
    syscall
    
    cmp rax, [brk_addr]
    jbe .fail               ; Failed to grow heap
    
    ; === Allocate mmap region ===
    lea rsi, [msg_mmap]
    mov rdx, msg_mmap_len
    call print_msg
    
    mov rax, 9              ; sys_mmap
    xor rdi, rdi            ; addr = 0 (kernel chooses)
    mov rsi, 8192           ; length = 8KB
    mov rdx, 3              ; prot = PROT_READ | PROT_WRITE
    mov r10, 0x22           ; flags = MAP_PRIVATE | MAP_ANONYMOUS
    mov r8, -1              ; fd = -1 (anonymous)
    xor r9, r9              ; offset = 0
    syscall
    
    test rax, rax
    js .fail                ; Negative means error
    test rax, rax
    jz .fail                ; Zero is also invalid
    mov [mmap_addr], rax    ; Save mmap address
    
    ; === Write test pattern to heap ===
    lea rsi, [msg_write_parent]
    mov rdx, msg_write_parent_len
    call print_msg
    
    mov rdi, [brk_addr]     ; Heap address
    mov rcx, 1024           ; Write 1024 qwords
    mov rax, 0xDEADBEEF     ; Pattern for heap
.write_heap:
    mov [rdi], rax
    add rdi, 8
    loop .write_heap
    
    ; === Write test pattern to mmap ===
    mov rdi, [mmap_addr]    ; Mmap address
    mov rcx, 1024           ; Write 1024 qwords
    mov rax, 0xCAFEBABE     ; Pattern for mmap
.write_mmap:
    mov [rdi], rax
    add rdi, 8
    loop .write_mmap
    
    ; === Fork ===
    lea rsi, [msg_fork]
    mov rdx, msg_fork_len
    call print_msg
    
    mov rax, 57             ; sys_fork
    syscall
    
    test rax, rax
    js .fail                ; Fork failed
    test rax, rax
    jz .child               ; Child process
    
    ; === Parent process ===
    mov [child_pid], rax    ; Save child PID
    
    lea rsi, [msg_parent]
    mov rdx, msg_parent_len
    call print_msg
    
    ; Wait for child
    lea rsi, [msg_parent_wait]
    mov rdx, msg_parent_wait_len
    call print_msg
    
    mov rax, 61             ; sys_wait4
    mov rdi, [child_pid]    ; Wait for specific child
    xor rsi, rsi            ; status_ptr = NULL
    xor rdx, rdx            ; options = 0
    syscall
    
    test rax, rax
    js .fail                ; Wait failed
    
    ; Verify parent's data is unchanged
    lea rsi, [msg_parent_verify]
    mov rdx, msg_parent_verify_len
    call print_msg
    
    ; Check heap
    mov rdi, [brk_addr]
    mov rcx, 1024
    mov rax, 0xDEADBEEF
.verify_parent_heap:
    cmp [rdi], rax
    jne .fail               ; Data was modified!
    add rdi, 8
    loop .verify_parent_heap
    
    ; Check mmap
    mov rdi, [mmap_addr]
    mov rcx, 1024
    mov rax, 0xCAFEBABE
.verify_parent_mmap:
    cmp [rdi], rax
    jne .fail               ; Data was modified!
    add rdi, 8
    loop .verify_parent_mmap
    
    ; Success!
    lea rsi, [msg_pass]
    mov rdx, msg_pass_len
    call print_msg
    
    mov rax, 60             ; sys_exit
    xor rdi, rdi            ; status = 0
    syscall

; === Child process ===
.child:
    lea rsi, [msg_child]
    mov rdx, msg_child_len
    call print_msg
    
    ; Modify heap data (should not affect parent)
    lea rsi, [msg_child_modify]
    mov rdx, msg_child_modify_len
    call print_msg
    
    mov rdi, [brk_addr]
    mov rcx, 1024
    mov rax, 0x12345678     ; Different pattern
.modify_child_heap:
    mov [rdi], rax
    add rdi, 8
    loop .modify_child_heap
    
    ; Modify mmap data (should not affect parent)
    mov rdi, [mmap_addr]
    mov rcx, 1024
    mov rax, 0x87654321     ; Different pattern
.modify_child_mmap:
    mov [rdi], rax
    add rdi, 8
    loop .modify_child_mmap
    
    ; Verify child's modifications
    mov rdi, [brk_addr]
    mov rcx, 1024
    mov rax, 0x12345678
.verify_child_heap:
    cmp [rdi], rax
    jne .fail               ; Child's write didn't work
    add rdi, 8
    loop .verify_child_heap
    
    mov rdi, [mmap_addr]
    mov rcx, 1024
    mov rax, 0x87654321
.verify_child_mmap:
    cmp [rdi], rax
    jne .fail               ; Child's write didn't work
    add rdi, 8
    loop .verify_child_mmap
    
    ; Child exits
    lea rsi, [msg_child_exit]
    mov rdx, msg_child_exit_len
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
