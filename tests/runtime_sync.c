#include "../crates/dream-mir/src/runtime/c/native/sync.c"
#include <assert.h>
#include <stdio.h>
#include <unistd.h>

void dream_future_fini(dream_ptr ptr) { (void)ptr; }
void dream_panic(dream_ptr message) {
    for (int i = 0; i < dream_str_len(message); ++i) {
        fputc(dream_char_at_u(message, i), stderr);
    }
    fputc('\n', stderr);
    exit(86);
}

typedef struct { dream_ptr target; int mode; int acquired; } Attempt;

static DREAM_THREAD_PROC(attempt) {
    Attempt *request = arg;
    if (request->mode == 1) {
        dream_lock_release(request->target);
    } else if (request->mode == 2) {
        int64_t start = dream_monotonic_ns();
        request->acquired = dream_lock_try_acquire_for(request->target, 10);
        assert(dream_monotonic_ns() - start >= 10000000);
    } else {
        dream_lock_acquire(request->target);
        request->acquired = 1;
        dream_lock_release(request->target);
    }
    return 0;
}

static void wait_for_waiter(dream_ptr target) {
    for (;;) {
        dream_mutex_lock(&locks_mu);
        LockState *state = lock_find(target);
        int waiting = state != NULL && state->waiters != 0;
        dream_mutex_unlock(&locks_mu);
        if (waiting) { return; }
        dream_thread_yield();
    }
}

static DREAM_THREAD_PROC(increment) {
    dream_ptr target = *(dream_ptr *)arg;
    for (int i = 0; i < 10000; ++i) {
        dream_lock_acquire(target);
        dream_lock_acquire(target);
        *(int64_t *)dream_p(target) += 1;
        dream_lock_release(target);
        dream_lock_release(target);
    }
    return 0;
}

int main(int argc, char **argv) {
    alarm(30);
    dream_thread worker;
    dream_ptr target = dream_malloc_shared(16, TAG_STRUCT_BASE);
    if (argc > 1) {
        if (strcmp(argv[1], "unheld") == 0) { dream_lock_release(target); }
        dream_lock_acquire(target);
        Attempt request = {target, strcmp(argv[1], "wrong-thread") == 0 ? 1 : 0, 0};
        assert(dream_thread_start(&worker, attempt, &request) == 0);
        if (strcmp(argv[1], "waiting-destroy") == 0) {
            wait_for_waiter(target);
            dream_release(target);
        }
        dream_thread_join(worker);
        return 1;
    }

    /* A dying object may hold an unbalanced manual lock, but has no remaining users. */
    dream_lock_acquire(target);
    dream_lock_acquire(target);
    dream_release(target);
    assert(HASH_COUNT(locks) == 0);
    dream_ptr reused = dream_malloc_shared(16, TAG_STRUCT_BASE);
    assert(reused == target);
    Attempt fresh = {reused, 0, 0};
    assert(dream_thread_start(&worker, attempt, &fresh) == 0);
    dream_thread_join(worker);
    assert(fresh.acquired);

    dream_lock_acquire(reused);
    Attempt timeout = {reused, 2, 0};
    assert(dream_thread_start(&worker, attempt, &timeout) == 0);
    dream_thread_join(worker);
    assert(!timeout.acquired);
    Attempt waiting = {reused, 0, 0};
    assert(dream_thread_start(&worker, attempt, &waiting) == 0);
    wait_for_waiter(reused);
    dream_lock_release(reused);
    dream_thread_join(worker);
    assert(waiting.acquired);
    dream_release(reused);
    assert(HASH_COUNT(locks) == 0);

    dream_ptr targets[2];
    dream_thread threads[6];
    for (int i = 0; i < 2; ++i) {
        targets[i] = dream_malloc_shared(16, TAG_STRUCT_BASE);
        *(int64_t *)dream_p(targets[i]) = 0;
    }
    for (int i = 0; i < 6; ++i) {
        assert(dream_thread_start(&threads[i], increment, &targets[i % 2]) == 0);
    }
    for (int i = 0; i < 6; ++i) { dream_thread_join(threads[i]); }
    assert(HASH_COUNT(locks) == 2);
    assert(&lock_find(targets[0])->changed != &lock_find(targets[1])->changed);
    for (int i = 0; i < 2; ++i) {
        assert(*(int64_t *)dream_p(targets[i]) == 30000);
        dream_release(targets[i]);
    }
    assert(HASH_COUNT(locks) == 0);
    for (int i = 0; i < 10000; ++i) {
        target = dream_malloc_shared(16, TAG_STRUCT_BASE);
        assert(dream_lock_try_acquire(target));
        dream_lock_release(target);
        dream_release(target);
        assert(HASH_COUNT(locks) == 0);
    }
    assert(debug_get_live_objects() == 0);
    puts("lock registry stress passed");
}
