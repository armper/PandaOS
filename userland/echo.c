/* echo.c - Minimal echo command implementation */

/* Simple write syscall */
static long sys_write(int fd, const void *buf, unsigned long count) {
    long ret;
    __asm__ volatile (
        "syscall"
        : "=a" (ret)
        : "a" (1), "D" (fd), "S" (buf), "d" (count)
        : "rcx", "r11", "memory"
    );
    return ret;
}

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

/* Simple strlen */
static unsigned long my_strlen(const char *s) {
    unsigned long len = 0;
    while (s[len]) len++;
    return len;
}

void _start(int argc, char **argv) {
    /* Skip program name (argv[0]) */
    for (int i = 1; i < argc; i++) {
        if (i > 1) {
            sys_write(1, " ", 1);
        }
        sys_write(1, argv[i], my_strlen(argv[i]));
    }
    sys_write(1, "\n", 1);
    sys_exit(0);
}
