#if !defined(_WIN32) && !defined(__APPLE__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE /* pthread_getattr_np */
#endif

#include "dream_platform_internal.h"

#include <stdint.h>
#include <stdio.h>

static const char OVERFLOW_MESSAGE[] = "panic: stack overflow\n";

/* Runs on an exhausted stack: no allocation, and stdout is flushed so the output that led to
 * the overflow is not lost, matching an ordinary panic. */
static void report_overflow(void) {
    fflush(stdout);
    dream_platform_current->write(2, OVERFLOW_MESSAGE, sizeof(OVERFLOW_MESSAGE) - 1, DREAM_TEXT_UTF8);
}

#ifdef _WIN32
#include <windows.h>

static LONG WINAPI on_exception(EXCEPTION_POINTERS *info) {
    if (info->ExceptionRecord->ExceptionCode == EXCEPTION_STACK_OVERFLOW) {
        report_overflow();
        dream_platform_abort();
    }
    return EXCEPTION_CONTINUE_SEARCH;
}

void dream_install_stack_guard(void) {
    /* The handler itself needs stack after the guard page is consumed. */
    ULONG reserve = 64 * 1024;
    SetThreadStackGuarantee(&reserve);
    AddVectoredExceptionHandler(1, on_exception);
}
#else
#include <pthread.h>
#include <signal.h>
#include <string.h>

/* A constant size: glibc's SIGSTKSZ is no longer a compile-time constant. */
#define ALT_STACK_SIZE (64 * 1024)
/* Faults this far below the stack's low end still hit its guard region; a frame larger than
 * the guard can skip past the first guard page. */
#define GUARD_REACH (1024 * 1024)

static char alt_stack[ALT_STACK_SIZE];
static uintptr_t stack_low;

static int is_overflow(const void *addr) {
    uintptr_t a = (uintptr_t)addr;
    return stack_low != 0 && a < stack_low + 4096 && a + GUARD_REACH >= stack_low;
}

static void on_fault(int sig, siginfo_t *info, void *context) {
    (void)context;
    if (is_overflow(info->si_addr)) {
        report_overflow();
        dream_platform_abort();
    }
    /* Not an overflow: returning re-executes the faulting access under the default action. */
    signal(sig, SIG_DFL);
}

static uintptr_t main_stack_low(void) {
#ifdef __APPLE__
    pthread_t self = pthread_self();
    return (uintptr_t)pthread_get_stackaddr_np(self) - pthread_get_stacksize_np(self);
#else
    pthread_attr_t attr;
    void *addr = NULL;
    size_t size = 0;
    if (pthread_getattr_np(pthread_self(), &attr) != 0) {
        return 0;
    }
    pthread_attr_getstack(&attr, &addr, &size);
    pthread_attr_destroy(&attr);
    return (uintptr_t)addr;
#endif
}

void dream_install_stack_guard(void) {
    stack_t ss;
    struct sigaction action;
    stack_low = main_stack_low();
    memset(&ss, 0, sizeof(ss));
    ss.ss_sp = alt_stack;
    ss.ss_size = sizeof(alt_stack);
    if (sigaltstack(&ss, NULL) != 0) {
        return;
    }
    memset(&action, 0, sizeof(action));
    action.sa_sigaction = on_fault;
    action.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigemptyset(&action.sa_mask);
    sigaction(SIGSEGV, &action, NULL);
    sigaction(SIGBUS, &action, NULL);
}
#endif
