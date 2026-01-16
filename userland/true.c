/* true.c - Minimal true command implementation */

/* Simple exit syscall */
static long sys_exit(int status) {
    long ret;
    __asm__ volatile (
        "syscall"
        : "=a" (ret)
        : "a" (60), "D" (status)
        : "rcx", "r11", "memory"
    );
    return ret;
}

void _start(void) {
    sys_exit(0);
}
