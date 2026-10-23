#include "dream_core.h"
#include "dream_thread.h"

#include <limits.h>
#include <stdlib.h>

static dream_mutex locks_mu = DREAM_MUTEX_INIT;
static _Thread_local void (*blocking_hook)(int entering);

void dream_thread_set_blocking_hook(void (*hook)(int entering)) { blocking_hook = hook; }

void dream_thread_blocking(int entering) {
    if (blocking_hook) { blocking_hook(entering); }
}

#define lock_failure(message) do { \
    dream_mutex_unlock(&locks_mu); \
    DREAM_PANIC_LITERAL(u##message); \
} while (0)

#define uthash_fatal(msg) lock_failure("panic: out of memory indexing locks")
#include "uthash.h"

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
    if (state->depth && !dream_thread_id_eq(state->owner, self)) {
        dream_thread_blocking(1);
        while (state->depth && !dream_thread_id_eq(state->owner, self)) {
            state->waiters += 1;
            dream_cond_wait(&state->changed, &locks_mu);
            state->waiters -= 1;
        }
        dream_thread_blocking(0);
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
    int waited = 0;
    while (state->depth && !dream_thread_id_eq(state->owner, self)) {
        int64_t remaining = deadline - dream_monotonic_ns();
        if (remaining <= 0) {
            if (waited) { dream_thread_blocking(0); }
            dream_mutex_unlock(&locks_mu);
            return 0;
        }
        if (!waited) {
            waited = 1;
            dream_thread_blocking(1);
        }
        state->waiters += 1;
        dream_cond_wait_ns(&state->changed, &locks_mu, remaining);
        state->waiters -= 1;
    }
    if (waited) { dream_thread_blocking(0); }
    lock_take(state, self);
    dream_mutex_unlock(&locks_mu);
    return 1;
}

void dream_semaphore_acquire(dream_ptr semaphore) {
    if (dream_semaphore_try_acquire(semaphore)) {
        return;
    }
    dream_thread_blocking(1);
    while (!dream_semaphore_try_acquire(semaphore)) {
        dream_thread_yield();
    }
    dream_thread_blocking(0);
}

void dream_semaphore_release(dream_ptr semaphore) {
    if (semaphore) {
        __atomic_fetch_add(dream_i32(semaphore), 1, __ATOMIC_RELEASE);
    }
}

int32_t dream_semaphore_try_acquire(dream_ptr semaphore) {
    return semaphore ? dream_permit_take(dream_i32(semaphore)) : 0;
}

int32_t dream_semaphore_try_acquire_for(dream_ptr semaphore, int32_t timeout_ms) {
    int64_t deadline;
    if (timeout_ms <= 0) {
        return dream_semaphore_try_acquire(semaphore);
    }
    if (dream_semaphore_try_acquire(semaphore)) {
        return 1;
    }
    deadline = monotonic_ms() + timeout_ms;
    dream_thread_blocking(1);
    int acquired = 1;
    while (!dream_semaphore_try_acquire(semaphore)) {
        if (monotonic_ms() >= deadline) {
            acquired = 0;
            break;
        }
        dream_thread_yield();
    }
    dream_thread_blocking(0);
    return acquired;
}

int32_t dream_js_call(int32_t target, int32_t via, dream_ptr method, int32_t argc) {
    (void)target;
    (void)via;
    (void)method;
    (void)argc;
    DREAM_PANIC_LITERAL(u"panic: JavaScript calls are unavailable in the native runtime");
}
