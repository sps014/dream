#include "include/dream_rt_native.h"
#include "include/dream_thread.h"

#include <limits.h>
#include <stdlib.h>

static dream_mutex locks_mu = DREAM_MUTEX_INIT;

#define lock_failure(message) do { \
    dream_mutex_unlock(&locks_mu); \
    DREAM_PANIC_LITERAL(u##message); \
} while (0)

#define uthash_fatal(msg) lock_failure("panic: out of memory indexing locks")
#include "include/uthash.h"

typedef struct LockState {
    dream_ptr target;
    dream_thread_id owner;
    int32_t depth;
    unsigned waiters;
    dream_cond changed;
    UT_hash_handle hh;
} LockState;

static LockState *locks;

static int64_t monotonic_ms(void) {
    return dream_monotonic_ns() / 1000000;
}

static LockState *lock_find(dream_ptr target) {
    LockState *state;
    HASH_FIND(hh, locks, &target, sizeof(target), state);
    return state;
}

static LockState *lock_state(dream_ptr target) {
    LockState *state = lock_find(target);
    if (state == NULL) {
        state = calloc(1, sizeof(*state));
        if (state == NULL) {
            lock_failure("panic: out of memory creating a lock");
        }
        state->target = target;
        dream_cond_init(&state->changed);
        HASH_ADD(hh, locks, target, sizeof(target), state);
    }
    return state;
}

static void lock_take(LockState *state, dream_thread_id self) {
    if (state->depth == INT32_MAX) {
        lock_failure("panic: lock recursion limit exceeded");
    }
    state->owner = self;
    state->depth += 1;
}

void dream_lock_forget(dream_ptr target) {
    dream_mutex_lock(&locks_mu);
    LockState *state = lock_find(target);
    if (state != NULL) {
        /* Waiters still reference the condition after releasing locks_mu. */
        if (state->waiters != 0) {
            lock_failure("panic: destroying a lock with waiting threads");
        }
        HASH_DEL(locks, state);
        dream_cond_destroy(&state->changed);
        free(state);
    }
    dream_mutex_unlock(&locks_mu);
}

void dream_lock_acquire(dream_ptr target) {
    if (!target) {
        return;
    }
    dream_thread_id self = dream_thread_self();
    dream_mutex_lock(&locks_mu);
    LockState *state = lock_state(target);
    while (state->depth && !dream_thread_id_eq(state->owner, self)) {
        state->waiters += 1;
        dream_cond_wait(&state->changed, &locks_mu);
        state->waiters -= 1;
    }
    lock_take(state, self);
    dream_mutex_unlock(&locks_mu);
}

void dream_lock_release(dream_ptr target) {
    if (!target) {
        return;
    }
    dream_mutex_lock(&locks_mu);
    LockState *state = lock_find(target);
    if (state == NULL || state->depth == 0
        || !dream_thread_id_eq(state->owner, dream_thread_self())) {
        lock_failure("panic: lock release requires the owning thread");
    }
    state->depth -= 1;
    if (state->depth == 0) {
        dream_cond_signal(&state->changed);
    }
    dream_mutex_unlock(&locks_mu);
}

int32_t dream_lock_try_acquire(dream_ptr target) {
    if (!target) {
        return 0;
    }
    dream_thread_id self = dream_thread_self();
    dream_mutex_lock(&locks_mu);
    LockState *state = lock_state(target);
    int32_t acquired = !state->depth || dream_thread_id_eq(state->owner, self);
    if (acquired) {
        lock_take(state, self);
    }
    dream_mutex_unlock(&locks_mu);
    return acquired;
}

int32_t dream_lock_try_acquire_for(dream_ptr target, int32_t timeout_ms) {
    if (timeout_ms <= 0) {
        return dream_lock_try_acquire(target);
    }
    if (!target) {
        return 0;
    }
    int64_t deadline = dream_monotonic_ns() + (int64_t)timeout_ms * 1000000;
    dream_thread_id self = dream_thread_self();
    dream_mutex_lock(&locks_mu);
    LockState *state = lock_state(target);
    while (state->depth && !dream_thread_id_eq(state->owner, self)) {
        int64_t remaining = deadline - dream_monotonic_ns();
        if (remaining <= 0) {
            dream_mutex_unlock(&locks_mu);
            return 0;
        }
        state->waiters += 1;
        dream_cond_wait_ns(&state->changed, &locks_mu, remaining);
        state->waiters -= 1;
    }
    lock_take(state, self);
    dream_mutex_unlock(&locks_mu);
    return 1;
}

void dream_semaphore_acquire(dream_ptr semaphore) {
    while (!dream_semaphore_try_acquire(semaphore)) {
        dream_thread_yield();
    }
}

void dream_semaphore_release(dream_ptr semaphore) {
    if (semaphore) {
        __atomic_fetch_add(dream_i32(semaphore), 1, __ATOMIC_RELEASE);
    }
}

int32_t dream_semaphore_try_acquire(dream_ptr semaphore) {
    int32_t permits;
    if (!semaphore) {
        return 0;
    }
    permits = __atomic_load_n(dream_i32(semaphore), __ATOMIC_ACQUIRE);
    while (permits > 0) {
        if (__atomic_compare_exchange_n(
                dream_i32(semaphore), &permits, permits - 1, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
            return 1;
        }
    }
    return 0;
}

int32_t dream_semaphore_try_acquire_for(dream_ptr semaphore, int32_t timeout_ms) {
    int64_t deadline;
    if (timeout_ms <= 0) {
        return dream_semaphore_try_acquire(semaphore);
    }
    deadline = monotonic_ms() + timeout_ms;
    while (!dream_semaphore_try_acquire(semaphore)) {
        if (monotonic_ms() >= deadline) {
            return 0;
        }
        dream_thread_yield();
    }
    return 1;
}

int32_t dream_js_call(int32_t target, int32_t via, dream_ptr method, int32_t argc) {
    (void)target;
    (void)via;
    (void)method;
    (void)argc;
    DREAM_PANIC_LITERAL(u"panic: JavaScript calls are unavailable in the native runtime");
}
