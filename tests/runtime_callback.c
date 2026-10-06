#include "../crates/dream-mir/src/runtime/c/core/include/dream_core.h"
#include "../crates/dream-mir/src/runtime/c/sys/native/include/dream_thread.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int dream_rt_mt;
static dream_thread_id owner;
static int destroyed;
static dream_mutex wake_mu = DREAM_MUTEX_INIT;
static unsigned wakes;

static void wake_owner(void *context) {
    assert(context == &wakes);
    dream_mutex_lock(&wake_mu);
    wakes += 1;
    dream_mutex_unlock(&wake_mu);
}

void dream_panic(dream_ptr message) {
    (void)message;
    fputs("callback owner panic\n", stderr);
    abort();
}

void dream_defer_drain_all(void) {}
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

int main(int argc, char **argv) {
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
    for (int round = 0; round < 1000; ++round) {
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
    dream_callback_owner_finish();
    puts("callback foreign release stress passed");
    return 0;
}
