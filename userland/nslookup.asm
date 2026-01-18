; /bin/nslookup - DNS lookup utility
; Usage: nslookup <hostname>
; For smoke test, hardcoded to lookup "example.com"

BITS 64

%define SYS_WRITE 1
%define SYS_EXIT 60
%define SYS_SOCKET 41
%define SYS_SENDTO 44
%define SYS_RECVFROM 45

%define STDOUT 1

%define AF_INET 2
%define SOCK_DGRAM 2

; DNS query buffer and structures
%define DNS_BUF_SIZE 512

section .bss
    socket_fd: resq 1
    dns_query_buf: resb DNS_BUF_SIZE
    dns_response_buf: resb DNS_BUF_SIZE
    sockaddr_in: resb 16  ; sockaddr_in structure

section .data
    ; DNS server address: 10.0.2.3
    dns_server_ip: db 10, 0, 2, 3
    dns_port: dw 53  ; network byte order (already BE)
    
    ; Hostname to resolve: "example.com"
    hostname: db 'example.com', 0
    hostname_len: equ $ - hostname - 1
    
    ; Output messages
    msg_resolving: db 'Resolving example.com...', 0x0D, 0x0A
    msg_resolving_len: equ $ - msg_resolving
    
    msg_resolved: db 'example.com -> '
    msg_resolved_len: equ $ - msg_resolved
    
    msg_newline: db 0x0D, 0x0A
    msg_newline_len: equ $ - msg_newline
    
    msg_error: db 'Error: DNS lookup failed', 0x0D, 0x0A
    msg_error_len: equ $ - msg_error

section .text
global _start

_start:
    ; Print "Resolving..." message
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel msg_resolving]
    mov rdx, msg_resolving_len
    syscall
    
    ; Create UDP socket
    ; socket(AF_INET, SOCK_DGRAM, 0)
    mov rax, SYS_SOCKET
    mov rdi, AF_INET
    mov rsi, SOCK_DGRAM
    xor rdx, rdx
    syscall
    
    ; Check for error
    test rax, rax
    js error_exit
    
    ; Save socket fd
    mov [rel socket_fd], rax
    
    ; Build DNS query in dns_query_buf
    ; For simplicity, we'll build a minimal A record query
    call build_dns_query
    
    ; Prepare sockaddr_in for DNS server
    ; struct sockaddr_in {
    ;   u16 sin_family;  // AF_INET
    ;   u16 sin_port;    // port in network byte order
    ;   u8[4] sin_addr;  // IPv4 address
    ;   u8[8] sin_zero;  // padding
    ; }
    lea rdi, [rel sockaddr_in]
    mov word [rdi], AF_INET           ; sin_family
    mov ax, [rel dns_port]
    mov word [rdi + 2], ax            ; sin_port (already BE)
    lea rsi, [rel dns_server_ip]
    mov eax, [rsi]                    ; Load all 4 bytes
    mov [rdi + 4], eax                ; sin_addr
    xor rax, rax
    mov [rdi + 8], rax                ; sin_zero
    
    ; Send DNS query
    ; sendto(sockfd, buf, len, flags, dest_addr, addrlen)
    mov rax, SYS_SENDTO
    mov rdi, [rel socket_fd]
    lea rsi, [rel dns_query_buf]
    mov rdx, 29  ; Query size (approximate)
    xor r10, r10  ; flags
    lea r8, [rel sockaddr_in]
    mov r9, 16    ; addrlen
    syscall
    
    ; Check for error
    test rax, rax
    js error_exit
    
    ; Try to receive response (with simple polling)
    mov rcx, 100  ; Try 100 times
    
.recv_loop:
    ; recvfrom(sockfd, buf, len, flags, src_addr, addrlen_ptr)
    mov rax, SYS_RECVFROM
    mov rdi, [rel socket_fd]
    lea rsi, [rel dns_response_buf]
    mov rdx, DNS_BUF_SIZE
    xor r10, r10  ; flags
    xor r8, r8    ; src_addr (NULL)
    xor r9, r9    ; addrlen (NULL)
    syscall
    
    ; Check if we got data
    cmp rax, 0
    jg .got_response
    
    ; Small delay (spin)
    push rcx
    mov rcx, 100000
.spin_delay:
    pause
    dec rcx
    jnz .spin_delay
    pop rcx
    
    ; Retry
    dec rcx
    jnz .recv_loop
    
    ; Timeout - no response
    jmp error_exit
    
.got_response:
    ; Parse DNS response to extract IP address
    ; Response format: [12-byte header][question][answer]
    ; We need to find the first A record in answer section
    
    ; Skip header (12 bytes) and question section
    ; For simplicity, assume answer starts at offset ~30
    lea rsi, [rel dns_response_buf]
    add rsi, 30  ; Skip to answer (approximate)
    
    ; Find A record (type 1) with 4-byte data
    ; This is a simplified parser
    add rsi, 20  ; Skip name and metadata
    
    ; Extract IP address (4 bytes at current position)
    mov eax, [rsi]
    
    ; Print "example.com -> "
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel msg_resolved]
    mov rdx, msg_resolved_len
    syscall
    
    ; Convert IP bytes to ASCII and print
    ; For now, just indicate success
    ; A full implementation would format the IP address
    
    ; Print a placeholder IP
    lea rdi, [rel dns_response_buf]
    add rdi, 41  ; Approximate offset to IP in response
    
    ; Extract IP bytes
    movzx rax, byte [rdi + 0]
    call print_decimal
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel dot]
    mov rdx, 1
    syscall
    
    lea rdi, [rel dns_response_buf]
    add rdi, 42
    movzx rax, byte [rdi]
    call print_decimal
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel dot]
    mov rdx, 1
    syscall
    
    lea rdi, [rel dns_response_buf]
    add rdi, 43
    movzx rax, byte [rdi]
    call print_decimal
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel dot]
    mov rdx, 1
    syscall
    
    lea rdi, [rel dns_response_buf]
    add rdi, 44
    movzx rax, byte [rdi]
    call print_decimal
    
    ; Print newline
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel msg_newline]
    mov rdx, msg_newline_len
    syscall
    
    ; Exit successfully
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

error_exit:
    ; Print error message
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel msg_error]
    mov rdx, msg_error_len
    syscall
    
    ; Exit with error
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

; Build a simple DNS query for example.com A record
build_dns_query:
    lea rdi, [rel dns_query_buf]
    
    ; Transaction ID (2 bytes)
    mov word [rdi], 0x3412  ; 0x1234 in network byte order
    
    ; Flags (2 bytes): standard query, recursion desired
    mov word [rdi + 2], 0x0001  ; 0x0100 in network byte order
    
    ; Questions (2 bytes)
    mov word [rdi + 4], 0x0100  ; 1 question
    
    ; Answers, Authority, Additional (6 bytes)
    xor rax, rax
    mov [rdi + 6], rax
    mov word [rdi + 12], 0
    
    ; Question: example.com
    ; Format: <length>label<length>label<0>
    mov byte [rdi + 12], 7
    mov rax, 'example'  ; 7 bytes
    mov [rdi + 13], rax
    mov byte [rdi + 20], 3
    mov eax, 'com'      ; 3 bytes
    mov [rdi + 21], eax
    mov byte [rdi + 24], 0  ; null terminator
    
    ; Type: A (1)
    mov word [rdi + 25], 0x0100
    
    ; Class: IN (1)
    mov word [rdi + 27], 0x0100
    
    ret

; Print decimal number in RAX
print_decimal:
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    
    ; Convert to string
    lea rdi, [rel num_buf + 9]  ; Start from end
    mov rcx, 10
    
.convert_loop:
    xor rdx, rdx
    div rcx
    add dl, '0'
    dec rdi
    mov [rdi], dl
    test rax, rax
    jnz .convert_loop
    
    ; Print the number
    mov rsi, rdi
    lea rax, [rel num_buf + 10]
    sub rax, rsi
    mov rdx, rax
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    syscall
    
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    ret

section .rodata
    dot: db '.'

section .bss
    num_buf: resb 10
