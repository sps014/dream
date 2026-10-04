#ifndef DREAM_THREAD_H
#define DREAM_THREAD_H

/* Threads, locks and a monotonic clock over pthreads, or Win32 where the MSVC target has no pthread.h. */

#include <stdint.h>

#if defined(_WIN32)

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <process.h>

typedef SRWLOCK dream_mutex;
typedef CONDITION_VARIABLE dream_cond;
typedef HANDLE dream_thread;
typedef DWORD dream_thread_id;

#define DREAM_MUTEX_INIT SRWLOCK_INIT
#define DREAM_COND_INIT CONDITION_VARIABLE_INIT
#define DREAM_THREAD_PROC(name) unsigned __stdcall name(void *arg)

static inline void dream_mutex_init(dream_mutex *m) { InitializeSRWLock(m); }
static inline void dream_mutex_destroy(dream_mutex *m) { (void)m; }
static inline void dream_mutex_lock(dream_mutex *m) { AcquireSRWLockExclusive(m); }
static inline void dream_mutex_unlock(dream_mutex *m) { ReleaseSRWLockExclusive(m); }

static inline void dream_cond_init(dream_cond *c) { InitializeConditionVariable(c); }
static inline void dream_cond_destroy(dream_cond *c) { (void)c; }
static inline void dream_cond_signal(dream_cond *c) { WakeConditionVariable(c); }
static inline void dream_cond_broadcast(dream_cond *c) { WakeAllConditionVariable(c); }
static inline void dream_cond_wait(dream_cond *c, dream_mutex *m) {
    SleepConditionVariableSRW(c, m, INFINITE, 0);
}
static inline void dream_cond_wait_ns(dream_cond *c, dream_mutex *m, int64_t rel_ns) {
    int64_t ms = (rel_ns + 999999) / 1000000;
    SleepConditionVariableSRW(c, m, ms >= (int64_t)INFINITE ? INFINITE - 1 : (DWORD)ms, 0);
}

static inline int dream_thread_start(dream_thread *t, unsigned(__stdcall *proc)(void *), void *arg) {
    *t = (HANDLE)_beginthreadex(NULL, 0, proc, arg, 0, NULL);
    return *t ? 0 : -1;
}
static inline void dream_thread_join(dream_thread t) {
    WaitForSingleObject(t, INFINITE);
    CloseHandle(t);
}
static inline void dream_thread_release(dream_thread t) { CloseHandle(t); }
static inline dream_thread_id dream_thread_self(void) { return GetCurrentThreadId(); }
static inline int dream_thread_id_eq(dream_thread_id a, dream_thread_id b) { return a == b; }
static inline void dream_thread_yield(void) { SwitchToThread(); }

static inline int64_t dream_monotonic_ns(void) {
    static LARGE_INTEGER freq;
    LARGE_INTEGER now;
    if (!freq.QuadPart) {
        QueryPerformanceFrequency(&freq);
    }
    QueryPerformanceCounter(&now);
    return (int64_t)((now.QuadPart / freq.QuadPart) * 1000000000LL
                     + (now.QuadPart % freq.QuadPart) * 1000000000LL / freq.QuadPart);
}

#else

#include <pthread.h>
#include <sched.h>
#include <time.h>

typedef pthread_mutex_t dream_mutex;
typedef pthread_cond_t dream_cond;
typedef pthread_t dream_thread;
typedef pthread_t dream_thread_id;

#define DREAM_MUTEX_INIT PTHREAD_MUTEX_INITIALIZER
#define DREAM_COND_INIT PTHREAD_COND_INITIALIZER
#define DREAM_THREAD_PROC(name) void *name(void *arg)

static inline void dream_mutex_init(dream_mutex *m) { pthread_mutex_init(m, NULL); }
static inline void dream_mutex_destroy(dream_mutex *m) { pthread_mutex_destroy(m); }
static inline void dream_mutex_lock(dream_mutex *m) { pthread_mutex_lock(m); }
static inline void dream_mutex_unlock(dream_mutex *m) { pthread_mutex_unlock(m); }

static inline void dream_cond_init(dream_cond *c) { pthread_cond_init(c, NULL); }
static inline void dream_cond_destroy(dream_cond *c) { pthread_cond_destroy(c); }
static inline void dream_cond_signal(dream_cond *c) { pthread_cond_signal(c); }
static inline void dream_cond_broadcast(dream_cond *c) { pthread_cond_broadcast(c); }
static inline void dream_cond_wait(dream_cond *c, dream_mutex *m) { pthread_cond_wait(c, m); }
/* pthread_cond_timedwait takes an absolute CLOCK_REALTIME deadline. */
static inline void dream_cond_wait_ns(dream_cond *c, dream_mutex *m, int64_t rel_ns) {
    struct timespec ts;
    int64_t nsec;
    clock_gettime(CLOCK_REALTIME, &ts);
    nsec = (int64_t)ts.tv_nsec + rel_ns % 1000000000LL;
    ts.tv_sec += (time_t)(rel_ns / 1000000000LL + nsec / 1000000000LL);
    ts.tv_nsec = (long)(nsec % 1000000000LL);
    pthread_cond_timedwait(c, m, &ts);
}

static inline int dream_thread_start(dream_thread *t, void *(*proc)(void *), void *arg) {
    return pthread_create(t, NULL, proc, arg);
}
static inline void dream_thread_join(dream_thread t) { pthread_join(t, NULL); }
static inline void dream_thread_release(dream_thread t) { pthread_detach(t); }
static inline dream_thread_id dream_thread_self(void) { return pthread_self(); }
static inline int dream_thread_id_eq(dream_thread_id a, dream_thread_id b) { return pthread_equal(a, b); }
static inline void dream_thread_yield(void) { sched_yield(); }

static inline int64_t dream_monotonic_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000LL + (int64_t)ts.tv_nsec;
}

#endif

#endif
