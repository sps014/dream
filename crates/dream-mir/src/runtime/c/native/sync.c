#include "include/dream_rt_native.h"

#include <pthread.h>
#include <sched.h>
#include <stdlib.h>
#include <time.h>

typedef struct LockState {
    dream_ptr target;
    pthread_t owner;
    int32_t depth;
    struct LockState *next;
} LockState;

static LockState *locks;
static pthread_mutex_t locks_mu = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t locks_changed = PTHREAD_COND_INITIALIZER;

static int64_t monotonic_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

static LockState *lock_state(dream_ptr target) {
    LockState *state;
    for (state = locks; state; state = state->next) {
        if (state->target == target) {
            return state;
        }
    }
    state = calloc(1, sizeof(*state));
    if (!state) {
        abort();
    }
    state->target = target;
    state->next = locks;
    locks = state;
    return state;
}

void dream_lock_acquire(dream_ptr lock_addr) {
    LockState *state;
    pthread_t self;
    if (!lock_addr) {
        return;
    }
    self = pthread_self();
    pthread_mutex_lock(&locks_mu);
    state = lock_state(lock_addr);
    while (state->depth && !pthread_equal(state->owner, self)) {
        pthread_cond_wait(&locks_changed, &locks_mu);
    }
    state->owner = self;
    state->depth += 1;
    pthread_mutex_unlock(&locks_mu);
}

void dream_lock_release(dream_ptr lock_addr) {
    LockState *state;
    if (!lock_addr) {
        return;
    }
    pthread_mutex_lock(&locks_mu);
    state = lock_state(lock_addr);
    if (state->depth && pthread_equal(state->owner, pthread_self())) {
        state->depth -= 1;
        if (!state->depth) {
            pthread_cond_broadcast(&locks_changed);
        }
    }
    pthread_mutex_unlock(&locks_mu);
}

int32_t dream_lock_try_acquire(dream_ptr lock_addr) {
    LockState *state;
    pthread_t self;
    int32_t acquired;
    if (!lock_addr) {
        return 0;
    }
    self = pthread_self();
    pthread_mutex_lock(&locks_mu);
    state = lock_state(lock_addr);
    acquired = !state->depth || pthread_equal(state->owner, self);
    if (acquired) {
        state->owner = self;
        state->depth += 1;
    }
    pthread_mutex_unlock(&locks_mu);
    return acquired;
}

int32_t dream_lock_try_acquire_for(dream_ptr lock_addr, int32_t timeout_ms) {
    int64_t deadline;
    if (timeout_ms <= 0) {
        return dream_lock_try_acquire(lock_addr);
    }
    deadline = monotonic_ms() + timeout_ms;
    while (!dream_lock_try_acquire(lock_addr)) {
        if (monotonic_ms() >= deadline) {
            return 0;
        }
        sched_yield();
    }
    return 1;
}

void dream_semaphore_acquire(dream_ptr semaphore) {
    while (!dream_semaphore_try_acquire(semaphore)) {
        sched_yield();
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
        sched_yield();
    }
    return 1;
}

dream_ptr dream_js_call(dream_ptr target, dream_ptr via, dream_ptr method, int32_t argc) {
    (void)target;
    (void)via;
    (void)method;
    (void)argc;
    abort();
    return 0;
}
