; Minimal shell for PandaOS
; REPL over serial using read/write syscalls

BITS 64

%define SYS_READ 0
%define SYS_WRITE 1
%define SYS_OPEN 2
%define SYS_CLOSE 3
%define SYS_PIPE 22
%define SYS_DUP2 33
%define SYS_KILL 37
%define SYS_FORK 57
%define SYS_EXECVE 59
%define SYS_EXIT 60
%define SYS_WAIT4 61
%define SYS_GETCWD 79
%define SYS_CHDIR 80
%define SYS_SETPGID 109

%define STDIN 0
%define STDOUT 1

%define SIGINT 2

%define O_RDONLY 0x0000
%define O_WRONLY 0x0001
%define O_CREAT 0x0040
%define O_TRUNC 0x0200
%define O_APPEND 0x0400

%define BUF_SIZE 128

section .text
global _start

_start:
    lea r13, [rel line_buf]

main_loop:
    ; write prompt
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel prompt]
    mov rdx, prompt_len
    syscall

    xor r12, r12

read_loop:
    ; read one byte
    mov rax, SYS_READ
    mov rdi, STDIN
    lea rsi, [rel input_char]
    mov rdx, 1
    syscall

    mov al, [rel input_char]
    
    ; Check for Ctrl+C (0x03)
    cmp al, 0x03
    je handle_ctrlc
    
    cmp al, 0x0D
    je line_done
    cmp al, 0x0A
    je line_done
    cmp al, 0x08
    je handle_backspace
    cmp al, 0x7F
    je handle_backspace

    cmp al, 0x20
    jb read_loop
    cmp al, 0x7E
    ja read_loop

    cmp r12, BUF_SIZE - 1
    jae read_loop

    mov [r13 + r12], al
    inc r12

    ; echo character
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel input_char]
    mov rdx, 1
    syscall
    jmp read_loop

handle_ctrlc:
    ; Check if there's a foreground process group
    mov rax, [rel foreground_pgid]
    test rax, rax
    jz ctrlc_no_fg
    
    ; Send SIGINT to the foreground process group
    ; kill(-pgid, SIGINT) - negative PID targets process group
    mov rax, SYS_KILL
    mov rdi, [rel foreground_pgid]
    neg rdi                       ; Negate to signal process group
    mov rsi, SIGINT
    syscall
    
    ; Clear foreground pgid
    mov qword [rel foreground_pgid], 0
    
    ; Print ^C and newline
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel ctrlc_msg]
    mov rdx, ctrlc_msg_len
    syscall
    
    ; Return to main loop to show prompt again
    jmp main_loop

ctrlc_no_fg:
    ; No foreground process, just clear the current input line
    xor r12, r12
    
    ; Print ^C and newline
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel ctrlc_msg]
    mov rdx, ctrlc_msg_len
    syscall
    
    ; Return to main loop to show prompt again
    jmp main_loop

handle_backspace:
    cmp r12, 0
    je read_loop
    dec r12
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel bs_seq]
    mov rdx, bs_seq_len
    syscall
    jmp read_loop

line_done:
    mov byte [r13 + r12], 0

    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, newline_len
    syscall

    cmp r12, 0
    je main_loop

    ; Check for redirection operators ('>', '>>', or '<')
    ; We'll support only one redirection per command
    xor rbx, rbx              ; rbx = position counter
    mov qword [rel redir_type], 0    ; 0 = none, 1 = input (<), 2 = output (>), 3 = append (>>)
    
check_redir_loop:
    cmp rbx, r12
    jae check_pipe_after_redir  ; No redirection found, check for pipe
    mov al, [r13 + rbx]
    cmp al, '<'
    je input_redir_found
    cmp al, '>'
    je check_append_or_output
    inc rbx
    jmp check_redir_loop

check_append_or_output:
    ; Check if next char is also '>' for append
    lea r8, [rbx + 1]
    cmp r8, r12
    jae output_redir_found  ; No next char, just output
    mov al, [r13 + r8]
    cmp al, '>'
    je append_redir_found
    jmp output_redir_found

input_redir_found:
    mov qword [rel redir_type], 1
    jmp process_redirection

append_redir_found:
    mov qword [rel redir_type], 3
    inc rbx  ; Skip second '>'
    jmp process_redirection

output_redir_found:
    mov qword [rel redir_type], 2
    ; jmp process_redirection (fallthrough)

process_redirection:
    ; rbx = position of last redirection operator character
    ; Null-terminate command before redirection
    mov byte [r13 + rbx], 0
    
    ; Find filename start (skip spaces after operator)
    lea r15, [r13 + rbx + 1]
skip_redir_spaces:
    mov al, [r15]
    cmp al, 0
    je redir_error
    cmp al, ' '
    jne found_redir_filename
    inc r15
    jmp skip_redir_spaces
    
found_redir_filename:
    ; r15 = start of filename
    ; Find end of filename (space or null)
    mov r14, r15
find_redir_end:
    mov al, [r14]
    cmp al, 0
    je redir_filename_done
    cmp al, ' '
    je redir_filename_done
    inc r14
    jmp find_redir_end
    
redir_filename_done:
    mov byte [r14], 0   ; Null-terminate filename
    mov [rel redir_file], r15  ; Save filename pointer
    
    ; Trim trailing spaces from command
    lea r8, [r13 + rbx]
    dec r8
trim_cmd_before_redir:
    cmp r8, r13
    jb check_pipe_after_redir
    mov al, [r8]
    cmp al, ' '
    jne check_pipe_after_redir
    mov byte [r8], 0
    dec r8
    jmp trim_cmd_before_redir

redir_error:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel redir_err]
    mov rdx, redir_err_len
    syscall
    jmp main_loop

check_pipe_after_redir:
    ; Check for pipe operator '|'
    xor rbx, rbx              ; rbx = position counter
check_pipe_loop:
    cmp rbx, r12
    jae no_pipe_found
    mov al, [r13 + rbx]
    cmp al, '|'
    je pipe_found
    inc rbx
    jmp check_pipe_loop

pipe_found:
    ; rbx contains position of '|'
    ; Split into left and right commands
    ; Left command: [r13 .. r13+rbx)
    ; Right command: [r13+rbx+1 .. r13+r12)
    
    ; Null-terminate left command
    mov byte [r13 + rbx], 0
    
    ; Find start of right command (skip spaces after '|')
    lea r15, [r13 + rbx + 1]  ; r15 = right command start
    xor r14, r14              ; r14 = right command length
    
skip_right_spaces:
    cmp r15, r13
    jae calc_right_len
    mov al, [r15]
    cmp al, 0
    je pipe_empty_error
    cmp al, ' '
    jne calc_right_len
    inc r15
    jmp skip_right_spaces
    
calc_right_len:
    mov r14, r13
    add r14, r12
    sub r14, r15              ; r14 = length of right command
    jz pipe_empty_error
    
    ; Trim trailing spaces from left command
    lea r8, [r13 + rbx]       ; r8 = end of left command
    dec r8
trim_left_loop:
    cmp r8, r13
    jb pipe_empty_error
    mov al, [r8]
    cmp al, ' '
    jne left_trimmed
    mov byte [r8], 0
    dec r8
    jmp trim_left_loop
    
left_trimmed:
    ; Skip leading spaces from left command
    mov r9, r13               ; r9 = left command start
skip_left_spaces:
    mov al, [r9]
    cmp al, 0
    je pipe_empty_error
    cmp al, ' '
    jne execute_pipeline
    inc r9
    jmp skip_left_spaces

pipe_empty_error:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel pipe_error]
    mov rdx, pipe_error_len
    syscall
    jmp main_loop

execute_pipeline:
    ; r9 = left command pointer
    ; r15 = right command pointer
    ; Save for later
    mov [rel left_cmd_ptr], r9
    mov [rel right_cmd_ptr], r15
    
    ; Create pipe: sys_pipe(pipefd)
    mov rax, SYS_PIPE
    lea rdi, [rel pipefd]
    syscall
    test rax, rax
    js pipe_syscall_error
    
    ; Fork left child
    mov rax, SYS_FORK
    syscall
    test rax, rax
    js fork_failed
    jz left_child
    
    ; Parent - save left PID
    ; The left child will become the process group leader
    mov [rel left_pid], rax
    mov [rel pipeline_pgid], rax  ; Save the pgid for the right child to join
    mov [rel foreground_pgid], rax    ; Set as foreground process group
    
    ; Fork right child
    mov rax, SYS_FORK
    syscall
    test rax, rax
    js fork_failed
    jz right_child
    
    ; Parent - save right PID
    mov [rel right_pid], rax
    
    ; Close both pipe ends in parent
    mov rax, SYS_CLOSE
    mov rdi, [rel pipefd]     ; close read end
    syscall
    
    mov rax, SYS_CLOSE
    mov rdi, [rel pipefd + 4] ; close write end
    syscall
    
    ; Wait for left child
wait_left:
    mov rax, SYS_WAIT4
    mov rdi, [rel left_pid]
    xor rsi, rsi
    xor rdx, rdx
    syscall
    test rax, rax
    js wait_left
    
    ; Wait for right child
wait_right:
    mov rax, SYS_WAIT4
    mov rdi, [rel right_pid]
    xor rsi, rsi
    xor rdx, rdx
    syscall
    test rax, rax
    js wait_right
    
    ; Clear foreground pgid after both children exit
    mov qword [rel foreground_pgid], 0
    
    jmp main_loop

left_child:
    ; Child: set itself as process group leader
    mov rax, SYS_SETPGID
    xor rdi, rdi              ; pid = 0 (current process)
    xor rsi, rsi              ; pgid = 0 (use own PID)
    syscall
    
    ; Redirect stdout to pipe write end
    mov rax, SYS_DUP2
    mov rdi, [rel pipefd + 4] ; write end
    mov rsi, STDOUT
    syscall
    
    ; Close both pipe ends
    mov rax, SYS_CLOSE
    mov rdi, [rel pipefd]
    syscall
    
    mov rax, SYS_CLOSE
    mov rdi, [rel pipefd + 4]
    syscall
    
    ; Exec left command
    mov rdi, [rel left_cmd_ptr]
    call resolve_and_exec
    
    ; If exec fails, exit
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

right_child:
    ; Child: join the left child's process group
    ; Get the pipeline pgid that parent saved (left child's PID)
    mov rax, SYS_SETPGID
    xor rdi, rdi              ; pid = 0 (current process)
    mov rsi, [rel pipeline_pgid]  ; pgid = left child's PID (group leader)
    syscall
    
    ; Redirect stdin from pipe read end
    mov rax, SYS_DUP2
    mov rdi, [rel pipefd]     ; read end
    mov rsi, STDIN
    syscall
    
    ; Close both pipe ends
    mov rax, SYS_CLOSE
    mov rdi, [rel pipefd]
    syscall
    
    mov rax, SYS_CLOSE
    mov rdi, [rel pipefd + 4]
    syscall
    
    ; Exec right command
    mov rdi, [rel right_cmd_ptr]
    call resolve_and_exec
    
    ; If exec fails, exit
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

pipe_syscall_error:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel pipe_error]
    mov rdx, pipe_error_len
    syscall
    jmp main_loop

no_pipe_found:
    ; Original command parsing continues here
    ; help
    cmp r12, 4
    jne check_exit
    mov al, [r13]
    cmp al, 'h'
    jne check_exit
    mov al, [r13 + 1]
    cmp al, 'e'
    jne check_exit
    mov al, [r13 + 2]
    cmp al, 'l'
    jne check_exit
    mov al, [r13 + 3]
    cmp al, 'p'
    jne check_exit
    jmp cmd_help

check_exit:
    cmp r12, 4
    jne check_echo
    mov al, [r13]
    cmp al, 'e'
    jne check_echo
    mov al, [r13 + 1]
    cmp al, 'x'
    jne check_echo
    mov al, [r13 + 2]
    cmp al, 'i'
    jne check_cd
    mov al, [r13 + 3]
    cmp al, 't'
    jne check_cd
    jmp cmd_exit

check_cd:
    cmp r12, 2
    jl check_echo
    mov al, [r13]
    cmp al, 'c'
    jne check_echo
    mov al, [r13 + 1]
    cmp al, 'd'
    jne check_echo
    
    ; cd command - builtin (no fork/exec)
    cmp r12, 2
    je cmd_cd_home       ; cd with no args -> go to /
    mov al, [r13 + 2]
    cmp al, ' '
    jne check_echo
    
    ; cd with argument
    lea rdi, [r13 + 3]   ; path starts at position 3
    mov rax, SYS_CHDIR
    syscall
    test rax, rax
    js cd_failed
    jmp main_loop      ; success, show new prompt
    
cd_failed:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel cd_err]
    mov rdx, cd_err_len
    syscall
    jmp main_loop

cmd_cd_home:
    ; cd with no args -> change to /
    lea rdi, [rel root_dir]
    mov rax, SYS_CHDIR
    syscall
    jmp main_loop

check_echo:
    cmp r12, 4
    jl check_cat
    mov al, [r13]
    cmp al, 'e'
    jne check_cat
    mov al, [r13 + 1]
    cmp al, 'c'
    jne check_cat
    mov al, [r13 + 2]
    cmp al, 'h'
    jne check_cat
    mov al, [r13 + 3]
    cmp al, 'o'
    jne check_cat

    cmp r12, 4
    je cmd_echo_empty
    mov al, [r13 + 4]
    cmp al, ' '
    jne check_cat

    lea rsi, [r13 + 5]
    mov rdx, r12
    sub rdx, 5
    jmp cmd_echo_arg

check_cat:
    cmp r12, 3
    jl check_true
    mov al, [r13]
    cmp al, 'c'
    jne check_true
    mov al, [r13 + 1]
    cmp al, 'a'
    jne check_true
    mov al, [r13 + 2]
    cmp al, 't'
    jne check_true

    cmp r12, 3
    je cmd_cat_usage
    mov al, [r13 + 3]
    cmp al, ' '
    jne check_true
    cmp r12, 4
    je cmd_cat_usage

    lea rsi, [r13 + 4]
    lea rdi, [rel cat_path]
    jmp cmd_fork_exec

check_true:
    cmp r12, 4
    jl cmd_unknown
    mov al, [r13]
    cmp al, 't'
    jne cmd_unknown
    mov al, [r13 + 1]
    cmp al, 'r'
    jne cmd_unknown
    mov al, [r13 + 2]
    cmp al, 'u'
    jne cmd_unknown
    mov al, [r13 + 3]
    cmp al, 'e'
    jne cmd_unknown
    cmp r12, 4
    jne cmd_unknown

    ; Run /bin/true
    xor rsi, rsi
    lea rdi, [rel true_path]
    jmp cmd_fork_exec

cmd_help:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel help_text]
    mov rdx, help_len
    syscall
    jmp main_loop

cmd_echo_empty:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, newline_len
    syscall
    jmp main_loop

cmd_echo_arg:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    syscall

    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel newline]
    mov rdx, newline_len
    syscall
    jmp main_loop

cmd_exit:
    mov rax, SYS_EXIT
    xor rdi, rdi
    syscall

cmd_unknown:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel unknown_text]
    mov rdx, unknown_len
    syscall
    jmp main_loop

cmd_cat_usage:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel cat_usage]
    mov rdx, cat_usage_len
    syscall
    jmp main_loop

cmd_fork_exec:
    ; rdi = program path
    ; rsi = argument (or 0 for none)
    ; Save these across fork
    mov r14, rdi
    mov r15, rsi

    ; fork()
    mov rax, SYS_FORK
    syscall
    test rax, rax
    js fork_failed
    jz child_process

    ; Parent process - set foreground pgid and wait for child
    mov r12, rax  ; Save child PID
    mov [rel foreground_pgid], rax  ; Set as foreground
    
parent_wait:
    mov rax, SYS_WAIT4
    mov rdi, r12  ; Wait for specific child
    xor rsi, rsi  ; Don't care about status
    xor rdx, rdx  ; options = 0
    syscall
    test rax, rax
    js parent_wait  ; If error (EAGAIN/EINTR), retry
    
    ; Child exited, clear foreground and continue shell
    mov qword [rel foreground_pgid], 0
    jmp main_loop

child_process:
    ; Child process - set itself as process group leader
    mov rax, SYS_SETPGID
    xor rdi, rdi              ; pid = 0 (current process)
    xor rsi, rsi              ; pgid = 0 (use own PID)
    syscall
    
    ; Handle redirection if present
    mov rax, [rel redir_type]
    cmp rax, 0
    je child_exec  ; No redirection
    
    ; Open the redirection file
    mov rdi, [rel redir_file]  ; filename
    
    cmp rax, 1
    je child_redir_input
    
    cmp rax, 3
    je child_redir_append
    
    ; Output redirection (>)
    ; open(filename, O_WRONLY | O_CREAT | O_TRUNC, 0644)
    mov rax, SYS_OPEN         ; SYS_OPEN
    ; rdi already has filename
    mov rsi, O_WRONLY | O_CREAT | O_TRUNC  ; flags
    mov rdx, 0o644            ; mode (octal)
    syscall
    test rax, rax
    js child_redir_failed
    
    ; dup2(fd, STDOUT)
    mov rdi, rax
    mov rsi, STDOUT
    mov rax, SYS_DUP2
    syscall
    test rax, rax
    js child_redir_failed
    
    ; Close original fd
    mov rax, SYS_CLOSE
    ; rdi already has the fd
    syscall
    
    jmp child_exec
    
child_redir_append:
    ; Append redirection (>>)
    ; open(filename, O_WRONLY | O_CREAT | O_APPEND, 0644)
    mov rax, SYS_OPEN         ; SYS_OPEN
    ; rdi already has filename
    mov rsi, O_WRONLY | O_CREAT | O_APPEND  ; flags
    mov rdx, 0o644            ; mode (octal)
    syscall
    test rax, rax
    js child_redir_failed
    
    ; dup2(fd, STDOUT)
    mov rdi, rax
    mov rsi, STDOUT
    mov rax, SYS_DUP2
    syscall
    test rax, rax
    js child_redir_failed
    
    ; Close original fd
    mov rax, SYS_CLOSE
    ; rdi already has the fd
    syscall
    
    jmp child_exec
    
child_redir_input:
    ; Input redirection (<)
    ; open(filename, O_RDONLY, 0)
    mov rax, SYS_OPEN         ; SYS_OPEN
    ; rdi already has filename
    mov rsi, O_RDONLY         ; flags
    mov rdx, 0
    syscall
    test rax, rax
    js child_redir_failed
    
    ; dup2(fd, STDIN)
    mov rdi, rax
    mov rsi, STDIN
    mov rax, SYS_DUP2
    syscall
    test rax, rax
    js child_redir_failed
    
    ; Close original fd
    mov rax, SYS_CLOSE
    ; rdi already has the fd
    syscall
    
    jmp child_exec

child_redir_failed:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel redir_fail]
    mov rdx, redir_fail_len
    syscall
    
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

child_exec:
    ; Exec the program
    mov rax, SYS_EXECVE
    mov rdi, r14
    mov rsi, r15
    xor rdx, rdx
    syscall
    
    ; If exec returns, it failed
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel exec_fail]
    mov rdx, exec_fail_len
    syscall
    
    ; Exit child with error
    mov rax, SYS_EXIT
    mov rdi, 1
    syscall

fork_failed:
    mov rax, SYS_WRITE
    mov rdi, STDOUT
    lea rsi, [rel fork_fail]
    mov rdx, fork_fail_len
    syscall
    jmp main_loop

; resolve_and_exec: Resolves a command and execs it
; Input: rdi = command string (e.g., "echo hello" or "wc")
; Parses into program path and argument, then execs
resolve_and_exec:
    ; Parse command into program and argument
    ; Find first space
    mov rsi, rdi
    xor rcx, rcx
find_space:
    mov al, [rsi + rcx]
    cmp al, 0
    je no_arg_in_cmd
    cmp al, ' '
    je found_space_in_cmd
    inc rcx
    jmp find_space

found_space_in_cmd:
    ; Null-terminate the command name
    mov byte [rsi + rcx], 0
    ; Argument starts after space
    lea rbx, [rsi + rcx + 1]
    ; Skip leading spaces in argument
skip_arg_spaces:
    mov al, [rbx]
    cmp al, 0
    je no_arg_in_cmd
    cmp al, ' '
    jne have_arg
    inc rbx
    jmp skip_arg_spaces

have_arg:
    ; rsi = command name, rbx = argument
    mov r14, rsi
    mov r15, rbx
    jmp resolve_cmd

no_arg_in_cmd:
    mov r14, rdi
    xor r15, r15

resolve_cmd:
    ; Check if command is an absolute path
    mov al, [r14]
    cmp al, '/'
    je exec_direct
    
    ; Try prepending /bin/
    lea rdi, [rel cmd_buf]
    lea rsi, [rel bin_prefix]
    mov rcx, bin_prefix_len
    rep movsb
    
    ; Copy command name
    mov rsi, r14
copy_cmd_name:
    mov al, [rsi]
    cmp al, 0
    je cmd_copied
    mov [rdi], al
    inc rsi
    inc rdi
    jmp copy_cmd_name

cmd_copied:
    mov byte [rdi], 0
    lea r14, [rel cmd_buf]

exec_direct:
    ; Execute with r14 = path, r15 = arg or 0
    mov rax, SYS_EXECVE
    mov rdi, r14
    mov rsi, r15
    xor rdx, rdx
    syscall
    ; If we get here, exec failed
    ret

section .rodata
prompt: db "panda> "
prompt_len equ $ - prompt
help_text: db "commands: help, echo, cat, cd, ls, true, exit", 0x0D, 0x0A
help_len equ $ - help_text
unknown_text: db "command not found", 0x0D, 0x0A
unknown_len equ $ - unknown_text
bs_seq: db 0x08, ' ', 0x08
bs_seq_len equ $ - bs_seq
newline: db 0x0D, 0x0A
newline_len equ $ - newline
ctrlc_msg: db "^C", 0x0D, 0x0A
ctrlc_msg_len equ $ - ctrlc_msg
cat_usage: db "usage: cat <path>", 0x0D, 0x0A
cat_usage_len equ $ - cat_usage
cd_err: db "cd: directory not found", 0x0D, 0x0A
cd_err_len equ $ - cd_err
exec_fail: db "exec failed", 0x0D, 0x0A
exec_fail_len equ $ - exec_fail
fork_fail: db "fork failed", 0x0D, 0x0A
fork_fail_len equ $ - fork_fail
pipe_error: db "pipe syntax error", 0x0D, 0x0A
pipe_error_len equ $ - pipe_error
redir_err: db "redirection syntax error", 0x0D, 0x0A
redir_err_len equ $ - redir_err
redir_fail: db "redirection failed", 0x0D, 0x0A
redir_fail_len equ $ - redir_fail
cat_path: db "/bin/cat", 0
true_path: db "/bin/true", 0
bin_prefix: db "/bin/"
bin_prefix_len equ $ - bin_prefix
root_dir: db "/", 0

section .bss
line_buf: resb BUF_SIZE
input_char: resb 1
pipefd: resd 2
left_pid: resq 1
right_pid: resq 1
left_cmd_ptr: resq 1
right_cmd_ptr: resq 1
cmd_buf: resb 64
foreground_pgid: resq 1
pipeline_pgid: resq 1
redir_type: resq 1      ; 0 = none, 1 = input (<), 2 = output (>)
redir_file: resq 1      ; pointer to filename
