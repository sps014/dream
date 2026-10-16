#include "dream_core.h"
#include "dream_thread.h"
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>

typedef struct Node { dream_ptr f; int64_t due; struct Node *next; } Node;
static _Thread_local int owner_thread;
static unsigned dropped, published, wakes, node_live;
static void (*enqueue_hook)(void);
int dream_rt_mt;
static int32_t *i32_at(dream_ptr f, int32_t off) { return (int32_t *)((char *)dream_p(f) + off); }
static dream_ptr *ptr_at(dream_ptr f, int32_t off) { return (dream_ptr *)((char *)dream_p(f) + off); }
static dream_result *result_at(dream_ptr f) { return (dream_result *)((char *)dream_p(f) + F_RESULT); }
static Node *foreign_node_take(void) { Node *n = calloc(1, sizeof(*n)); assert(n); __atomic_fetch_add(&node_live, 1, __ATOMIC_RELAXED); return n; }
static void foreign_node_put(Node *n) { __atomic_fetch_sub(&node_live, 1, __ATOMIC_RELAXED); free(n); }
void dream_free(dream_ptr ptr) { (void)ptr; abort(); }
int dream_rc_last_observed(dream_ptr ptr) { (void)ptr; abort(); }
void dream_panic(dream_ptr message) { (void)message; abort(); }
void dream_publish(dream_ptr f) {
    assert(f);
    __atomic_fetch_or(dream_tag_word(f), TAG_SHARED, __ATOMIC_RELAXED);
    int32_t *rc = dream_rc_word(f);
    int32_t count = __atomic_load_n(rc, __ATOMIC_RELAXED);
    while (count > 0 && !__atomic_compare_exchange_n(rc, &count,
        count | DREAM_RC_SHARED_BIT, 0, __ATOMIC_RELAXED, __ATOMIC_RELAXED)) {}
    __atomic_fetch_add(&published, 1, __ATOMIC_RELAXED);
}
int dream_rc_last_slow(int32_t *rc, int32_t value) {
    for (;;) {
        uint32_t count = (uint32_t)value & INT32_MAX;
        assert(value < 0 && value != DREAM_RC_IMMORTAL && count);
        int32_t next = count == 1 ? 0 : (int32_t)((uint32_t)value - 1);
        if (__atomic_compare_exchange_n(rc, &value, next, 0, __ATOMIC_RELAXED, __ATOMIC_RELAXED)) {
            return count == 1;
        }
    }
}
void dream_enqueue(dream_ptr f) {
    assert(owner_thread && f);
    __atomic_fetch_add(&wakes, 1, __ATOMIC_RELAXED);
    if (enqueue_hook) { enqueue_hook(); }
}
static void scheduler_drop_start_retain(dream_ptr f) {
    assert(owner_thread && i32_at(f, F_NEXT)[0] == 1);
    i32_at(f, F_NEXT)[0] = 2;
    dream_rc_last_raw(f);
    __atomic_fetch_add(&dropped, 1, __ATOMIC_RELAXED);
}
static void dream_combinator_progress(dream_ptr w, dream_ptr child) { (void)w; (void)child; abort(); }
#include "../crates/dream-mir/src/runtime/c/sys/native/foreign_queue.h"

static dream_ptr frame(void) {
    void *p = calloc(1, NATIVE_HEAP_HEADER_SIZE + F_SLOTS); assert(p);
    dream_ptr f = (dream_ptr)((char *)p + NATIVE_HEAP_HEADER_SIZE);
    *dream_rc_word(f) = 2;
    *dream_tag_word(f) = TAG_FUTURE;
    i32_at(f, F_POLL)[0] = 1;
    i32_at(f, F_NEXT)[0] = 1;
    return f;
}

static void drop_owner(dream_ptr f) {
    assert(i32_at(f, F_POLL)[0]);
    i32_at(f, F_POLL)[0] = 0;
    assert(!dream_rc_last_raw(f));
}
static void frame_free(dream_ptr f) {
    if (i32_at(f, F_NEXT)[0] == 1) { scheduler_drop_start_retain(f); }
    if (i32_at(f, F_POLL)[0]) { assert(dream_rc_last_raw(f)); }
    assert(__atomic_load_n(dream_rc_word(f), __ATOMIC_RELAXED) == 0);
    free((char *)dream_p(f) - NATIVE_HEAP_HEADER_SIZE);
}

struct batch { dream_ptr frames[8]; };
static DREAM_THREAD_PROC(complete_batch) {
    struct batch *batch = arg;
    int64_t until = dream_monotonic_ns() + INT64_C(2000000);
    while (dream_monotonic_ns() < until) { dream_thread_yield(); }
    for (unsigned i = 0; i < 8; i++) {
        dream_complete_foreign(batch->frames[i], i * 3 + 1);
        dream_complete_foreign(batch->frames[i], 999);
    }
    return 0;
}
static DREAM_THREAD_PROC(scheduler_stress) {
    (void)arg;
    owner_thread = 1;
    assert(!foreign_outstanding());
    for (unsigned round = 0; round < 32; round++) {
        struct batch batch;
        for (unsigned i = 0; i < 8; i++) {
            batch.frames[i] = frame();
            dream_foreign_work_begin(batch.frames[i]);
            assert(dream_tag_shared(batch.frames[i]));
            if ((i & 3u) == 1) {
                dream_mutex_lock(&foreign_claim_mu);
                i32_at(batch.frames[i], F_STATUS)[0] = 2;
                dream_mutex_unlock(&foreign_claim_mu);
            }
        }
        dream_thread thread;
        assert(dream_thread_start(&thread, complete_batch, &batch) == 0);
        for (unsigned i = 0; i < 8; i++) { drop_owner(batch.frames[i]); }
        dream_thread_join(thread);
        assert(foreign_outstanding());
        foreign_drain();
        assert(!foreign_outstanding());
        for (unsigned i = 0; i < 8; i++) {
            dream_ptr f = batch.frames[i];
            assert(!*scheduler_at(f));
            assert(i32_at(f, F_NEXT)[0] == 2);
            if ((i & 3u) == 1) {
                assert(i32_at(f, F_STATUS)[0] == 2 && *result_at(f) == 0);
            } else {
                assert(i32_at(f, F_STATUS)[0] == 1 && *result_at(f) == i * 3 + 1);
            }
            frame_free(f);
        }
    }
    return 0;
}

static int prepared, proceed, drained, callback_done;
static DREAM_THREAD_PROC(independent_drain) {
    (void)arg;
    owner_thread = 1;
    dream_ptr f = frame();
    dream_foreign_work_begin(f);
    dream_complete_foreign(f, 31);
    __atomic_store_n(&prepared, 1, __ATOMIC_RELEASE);
    while (!__atomic_load_n(&proceed, __ATOMIC_ACQUIRE)) { dream_thread_yield(); }
    foreign_drain();
    assert(!foreign_outstanding() && i32_at(f, F_NEXT)[0] == 2);
    frame_free(f);
    __atomic_store_n(&drained, 1, __ATOMIC_RELEASE);
    return 0;
}
static DREAM_THREAD_PROC(independent_wake) {
    callback_wake(arg);
    __atomic_store_n(&callback_done, 1, __ATOMIC_RELEASE);
    return 0;
}
static int wait_flag(int *flag) {
    int64_t deadline = dream_monotonic_ns() + INT64_C(2000000000);
    while (!__atomic_load_n(flag, __ATOMIC_ACQUIRE) && dream_monotonic_ns() < deadline) {
        dream_thread_yield();
    }
    return __atomic_load_n(flag, __ATOMIC_ACQUIRE);
}

static dream_ptr during_drain;
static void complete_during_drain(void) {
    enqueue_hook = NULL;
    dream_complete_foreign(during_drain, 43);
}

static void drain_detached_batches(void) {
    dream_ptr frames[3] = {frame(), frame(), frame()};
    dream_ptr waker = frame();
    unsigned previous_wakes = wakes;
    for (unsigned i = 0; i < 3; ++i) {
        *ptr_at(frames[i], F_WAKER) = waker;
        dream_foreign_work_begin(frames[i]);
    }
    dream_complete_foreign(frames[0], 41);
    dream_complete_foreign(frames[1], 42);
    during_drain = frames[2];
    enqueue_hook = complete_during_drain;
    foreign_drain();
    assert(!foreign_outstanding() && wakes == previous_wakes + 3 && node_live == 0);
    for (unsigned i = 0; i < 3; ++i) {
        assert(*result_at(frames[i]) == 41 + i && !*ptr_at(frames[i], F_WAKER));
        frame_free(frames[i]);
    }
    frame_free(waker);
}
int main(void) {
    owner_thread = 1;
    dream_ptr f = frame();
    dream_foreign_work_begin(f);
    assert(foreign_outstanding());
    dream_foreign_work_end(f);
    dream_foreign_work_end(f);
    assert(!foreign_outstanding() && !*scheduler_at(f));
    dream_complete_foreign(f, 999);
    assert(!foreign_outstanding());
    frame_free(f);
    dream_ptr waker = frame();
    f = frame();
    *ptr_at(f, F_WAKER) = waker;
    dream_foreign_work_begin(f);
    dream_complete_foreign(f, 17);
    foreign_drain();
    assert(wakes == 1 && !*ptr_at(f, F_WAKER));
    frame_free(f); frame_free(waker);
    drain_detached_batches();

    dream_thread drain_thread, wake_thread;
    assert(dream_thread_start(&drain_thread, independent_drain, NULL) == 0);
    assert(wait_flag(&prepared));
    dream_mutex_lock(&foreign_claim_mu);
    __atomic_store_n(&proceed, 1, __ATOMIC_RELEASE);
    assert(dream_thread_start(&wake_thread, independent_wake, &sched) == 0);
    int drain_ok = wait_flag(&drained);
    int wake_ok = wait_flag(&callback_done);
    dream_mutex_unlock(&foreign_claim_mu);
    dream_thread_join(drain_thread); dream_thread_join(wake_thread);
    assert(drain_ok && wake_ok);

    dream_thread threads[16];
    for (unsigned i = 0; i < 16; i++) { assert(dream_thread_start(&threads[i], scheduler_stress, NULL) == 0); }
    for (unsigned i = 0; i < 16; i++) { dream_thread_join(threads[i]); }
    assert(dropped == 4104 && published == 7179 && node_live == 0);
    puts("foreign queue scheduler isolation passed");
    return 0;
}
