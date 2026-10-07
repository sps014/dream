#include "../crates/dream-mir/src/runtime/c/core/include/dream_core.h"
#include "../crates/dream-mir/src/runtime/c/sys/native/include/dream_thread.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int dream_rt_mt;
static dream_thread_id owner;
static int destroyed;
static dream_mutex wake_mu = DREAM_MUTEX_INIT;
static unsigned wakes;
static dream_mutex barrier_mu = DREAM_MUTEX_INIT;
static dream_cond barrier_cv = DREAM_COND_INIT;
static int pause_wake, wake_entered, wake_release, finish_entered, finished;
static void finish_barrier(void) {
    dream_mutex_lock(&barrier_mu);
    finish_entered = 1;
    dream_cond_broadcast(&barrier_cv);
    dream_mutex_unlock(&barrier_mu);
}


static void wake_owner(void *context) {
    assert(context == &wakes);
    dream_mutex_lock(&wake_mu);
    wakes += 1;
    dream_mutex_unlock(&wake_mu);
    dream_mutex_lock(&barrier_mu);
    if (pause_wake) {
        wake_entered = 1;
        dream_cond_broadcast(&barrier_cv);
        while (!wake_release) { dream_cond_wait(&barrier_cv, &barrier_mu); }
    }
    dream_mutex_unlock(&barrier_mu);
}

void dream_panic(dream_ptr message) {
    (void)message;
    fputs("callback owner panic\n", stderr);
    abort();
}

void dream_defer_drain_all(void) {}
void dream_cycle_retain(dream_ptr ptr) { (void)ptr; abort(); }
void dream_retain_slow(int32_t *rc, int32_t v) {
    (void)v;
    __atomic_fetch_add(rc, 1, __ATOMIC_RELAXED);
}

void dream_release_object(dream_ptr object) {
    assert(dream_thread_id_eq(owner, dream_thread_self()));
    int32_t *rc = dream_rc_word(object);
    assert(*rc > 0);
    if (--*rc == 0) {
        dream_callback_unregister(object);
        destroyed += 1;
        free(dream_p(object - NATIVE_HEAP_HEADER_SIZE));
    }
}

static dream_ptr callback_new(void) {
    char *block = (char *)calloc(1, NATIVE_HEAP_HEADER_SIZE + 8);
    assert(block);
    dream_ptr object = (dream_ptr)(block + NATIVE_HEAP_HEADER_SIZE);
    *dream_rc_word(object) = 1;
    dream_callback_register(object);
    return object;
}

static DREAM_THREAD_PROC(release_many) {
    dream_ptr object = *(dream_ptr *)arg;
    for (int i = 0; i < 8; ++i) {
        dream_callback_release(object);
    }
    return 0;
}

static DREAM_THREAD_PROC(wrong_owner) {
    dream_thread_attach();
    dream_callback_check(*(dream_ptr *)arg);
    return 0;
}

static DREAM_THREAD_PROC(release_once) {
    dream_callback_release(*(dream_ptr *)arg);
    return 0;
}
static DREAM_THREAD_PROC(unblock_wake) {
    (void)arg;
    dream_mutex_lock(&barrier_mu);
    while (!finish_entered) { dream_cond_wait(&barrier_cv, &barrier_mu); }
    assert(!finished);
    wake_release = 1;
    dream_cond_broadcast(&barrier_cv);
    dream_mutex_unlock(&barrier_mu);
    return 0;
}
#define DREAM_CALLBACK_TEST_FINISH finish_barrier
#include "../crates/dream-mir/src/runtime/c/sys/native/callback.c"

int main(int argc, char **argv) {
    alarm(30);
    dream_thread threads[8];
    owner = dream_thread_self();
    dream_thread_attach();
    dream_callback_set_waker(wake_owner, &wakes);
    if (argc > 1 && strcmp(argv[1], "wrong-owner") == 0) {
        dream_ptr object = callback_new();
        assert(dream_thread_start(&threads[0], wrong_owner, &object) == 0);
        dream_thread_join(threads[0]);
        return 1;
    }
    enum { stress_rounds = 100 };
    for (int round = 0; round < stress_rounds; ++round) {
        dream_ptr object = callback_new();
        for (int i = 0; i < 64; ++i) {
            dream_callback_retain(object);
        }
        for (int i = 0; i < 8; ++i) {
            assert(dream_thread_start(&threads[i], release_many, &object) == 0);
        }
        dream_callback_release(object);
        dream_callback_drain();
        for (int i = 0; i < 8; ++i) {
            dream_thread_join(threads[i]);
        }
        dream_callback_drain();
        assert(destroyed == round + 1);
        assert(wakes == (unsigned)(round + 1) * 64);
        assert(!dream_callback_pending());
    }
    pause_wake = 1;
    dream_ptr object = callback_new();
    dream_callback_retain(object);
    assert(dream_thread_start(&threads[0], release_once, &object) == 0);
    dream_mutex_lock(&barrier_mu);
    while (!wake_entered) { dream_cond_wait(&barrier_cv, &barrier_mu); }
    dream_mutex_unlock(&barrier_mu);
    dream_callback_release(object);
    assert(dream_thread_start(&threads[1], unblock_wake, NULL) == 0);
    dream_callback_owner_finish();
    finished = 1;
    dream_thread_join(threads[0]);
    dream_thread_join(threads[1]);
    assert(destroyed == stress_rounds + 1);
    puts("callback foreign release stress passed");
    return 0;
}
