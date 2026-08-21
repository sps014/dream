#include "dream_rt_wasm32.h"

#ifndef DREAM_WASM32_THREADS

void dream_lock_acquire(dream_ptr lock_addr) { (void)lock_addr; }

void dream_lock_release(dream_ptr lock_addr) { (void)lock_addr; }

int32_t dream_lock_try_acquire(dream_ptr lock_addr) {
    (void)lock_addr;
    return 1;
}

int32_t dream_lock_try_acquire_for(dream_ptr lock_addr, int32_t timeout_ms) {
    (void)lock_addr;
    (void)timeout_ms;
    return 1;
}

void dream_semaphore_acquire(dream_ptr semaphore) { (void)semaphore; }

void dream_semaphore_release(dream_ptr semaphore) { (void)semaphore; }

int32_t dream_semaphore_try_acquire(dream_ptr semaphore) {
    (void)semaphore;
    return 1;
}

int32_t dream_semaphore_try_acquire_for(dream_ptr semaphore, int32_t timeout_ms) {
    (void)semaphore;
    (void)timeout_ms;
    return 1;
}

#else

static int32_t *word(dream_ptr p) {
    return (int32_t *)(uintptr_t)(uint32_t)p;
}

static int32_t thread_id(void) { return dream_instance_tid(); }

static int32_t packed(int32_t tid) { return (tid << 16) | 1; }

int32_t dream_lock_try_acquire(dream_ptr lock_addr) {
    int32_t *addr = word(lock_addr);
    int32_t tid = thread_id();
    int32_t cur = __atomic_load_n(addr, __ATOMIC_ACQUIRE);
    int32_t expected;
    if (cur == 0) {
        expected = 0;
        return __atomic_compare_exchange_n(addr, &expected, packed(tid), 0,
                                           __ATOMIC_ACQUIRE, __ATOMIC_RELAXED);
    }
    if ((cur >> 16) == tid) {
        __atomic_fetch_add(addr, 1, __ATOMIC_RELAXED);
        return 1;
    }
    return 0;
}

void dream_lock_acquire(dream_ptr lock_addr) {
    int32_t *addr = word(lock_addr);
    int32_t tid = thread_id();
    for (;;) {
        int32_t cur = __atomic_load_n(addr, __ATOMIC_ACQUIRE);
        int32_t expected;
        if (cur == 0) {
            expected = 0;
            if (__atomic_compare_exchange_n(addr, &expected, packed(tid), 0,
                                            __ATOMIC_ACQUIRE, __ATOMIC_RELAXED)) {
                return;
            }
        } else if ((cur >> 16) == tid) {
            __atomic_fetch_add(addr, 1, __ATOMIC_RELAXED);
            return;
        }
    }
}

void dream_lock_release(dream_ptr lock_addr) {
    int32_t *addr = word(lock_addr);
    int32_t cur = __atomic_load_n(addr, __ATOMIC_ACQUIRE);
    if ((cur & 65535) == 1) {
        __atomic_store_n(addr, 0, __ATOMIC_RELEASE);
        (void)__builtin_wasm_memory_atomic_notify(addr, 1);
    } else {
        __atomic_fetch_sub(addr, 1, __ATOMIC_RELEASE);
    }
}

int32_t dream_lock_try_acquire_for(dream_ptr lock_addr, int32_t timeout_ms) {
    int32_t *addr = word(lock_addr);
    int64_t timeout_ns;
    if (timeout_ms <= 0) {
        return dream_lock_try_acquire(lock_addr);
    }
    timeout_ns = (int64_t)timeout_ms * 1000000;
    for (;;) {
        int32_t ok = dream_lock_try_acquire(lock_addr);
        int32_t cur;
        int32_t wait_res;
        if (ok) {
            return ok;
        }
        cur = __atomic_load_n(addr, __ATOMIC_ACQUIRE);
        wait_res = __builtin_wasm_memory_atomic_wait32(addr, cur, timeout_ns);
        if (wait_res == 2) {
            return dream_lock_try_acquire(lock_addr);
        }
    }
}

void dream_semaphore_acquire(dream_ptr semaphore) {
    int32_t *addr = word(semaphore);
    for (;;) {
        int32_t cur = __atomic_load_n(addr, __ATOMIC_ACQUIRE);
        int32_t expected;
        if (cur > 0) {
            expected = cur;
            if (__atomic_compare_exchange_n(addr, &expected, cur - 1, 0,
                                            __ATOMIC_ACQUIRE, __ATOMIC_RELAXED)) {
                return;
            }
        }
    }
}

void dream_semaphore_release(dream_ptr semaphore) {
    int32_t *addr = word(semaphore);
    __atomic_fetch_add(addr, 1, __ATOMIC_RELEASE);
    (void)__builtin_wasm_memory_atomic_notify(addr, 1);
}

int32_t dream_semaphore_try_acquire(dream_ptr semaphore) {
    int32_t *addr = word(semaphore);
    int32_t cur = __atomic_load_n(addr, __ATOMIC_ACQUIRE);
    int32_t expected;
    if (cur <= 0) {
        return 0;
    }
    expected = cur;
    return __atomic_compare_exchange_n(addr, &expected, cur - 1, 0, __ATOMIC_ACQUIRE,
                                       __ATOMIC_RELAXED);
}

int32_t dream_semaphore_try_acquire_for(dream_ptr semaphore, int32_t timeout_ms) {
    int32_t *addr = word(semaphore);
    int64_t timeout_ns;
    if (timeout_ms <= 0) {
        return dream_semaphore_try_acquire(semaphore);
    }
    timeout_ns = (int64_t)timeout_ms * 1000000;
    for (;;) {
        int32_t ok = dream_semaphore_try_acquire(semaphore);
        int32_t cur;
        int32_t wait_res;
        if (ok) {
            return ok;
        }
        cur = __atomic_load_n(addr, __ATOMIC_ACQUIRE);
        wait_res = __builtin_wasm_memory_atomic_wait32(addr, cur, timeout_ns);
        if (wait_res == 2) {
            return dream_semaphore_try_acquire(semaphore);
        }
    }
}

#endif

dream_ptr dream_js_call(dream_ptr target, dream_ptr via, dream_ptr method, int32_t argc) {
    (void)target;
    (void)via;
    (void)method;
    (void)argc;
    __builtin_trap();
    return 0;
}
