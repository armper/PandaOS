/* hello_musl.c - Simple test program for musl compatibility */
#include <unistd.h>

/* Simple write syscall wrapper */
static long syscall1(long number, long arg1) {
    long ret;
    __asm__ volatile (
        "syscall"
        : "=a" (ret)
        : "a" (number), "D" (arg1)
        : "rcx", "r11", "memory"
    );
    return ret;
}

static long syscall3(long number, long arg1, long arg2, long arg3) {
    long ret;
    __asm__ volatile (
        "syscall"
        : "=a" (ret)
        : "a" (number), "D" (arg1), "S" (arg2), "d" (arg3)
        : "rcx", "r11", "memory"
    );
    return ret;
}

/* Syscall numbers */
#define SYS_write 1
#define SYS_exit 60

void _start(void) {
    const char *msg = "Hello from musl libc!\n";
    long len = 0;
    
    /* Calculate string length */
    while (msg[len]) len++;
    
    /* write(1, msg, len) */
    syscall3(SYS_write, 1, (long)msg, len);
    
    /* exit(0) */
    syscall1(SYS_exit, 0);
    
    /* Never reached */
    while (1);
}
