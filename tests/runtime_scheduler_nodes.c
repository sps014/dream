#include "dream_core.h"
#include "dream_thread.h"
#include "dream_platform_internal.h"
#include <assert.h>
#include <stdlib.h>
#include <sched.h>
#include "../crates/dream-mir/src/runtime/c/sys/shared/node_pool.h"
#include "../crates/dream-mir/src/runtime/c/sys/shared/async_frame.h"
#include "../crates/dream-mir/src/runtime/c/sys/shared/timer_heap.h"

static dream_mutex mutex = DREAM_MUTEX_INIT;
static _Thread_local int locked;
static unsigned allocations, deallocations;
static void *allocate(size_t size) {
    assert(!locked);
    __atomic_fetch_add(&allocations, 1, __ATOMIC_RELAXED);
    return malloc(size);
}
static void deallocate(void *p) {
    assert(!locked);
    __atomic_fetch_add(&deallocations, 1, __ATOMIC_RELAXED);
    free(p);
}
static void lock(unsigned domain) {
    assert(domain == DREAM_LOCK_HEAP && !locked);
    dream_mutex_lock(&mutex);
    locked = 1;
}
static void unlock(unsigned domain) {
    assert(domain == DREAM_LOCK_HEAP && locked);
    locked = 0;
    dream_mutex_unlock(&mutex);
}
static const dream_platform platform = {.allocate = allocate, .deallocate = deallocate, .lock = lock, .unlock = unlock};
const dream_platform *dream_platform_current = &platform;
void dream_panic(dream_ptr message) { (void)message; abort(); }

static unsigned char frames[5000][F_SLOTS];
static dream_ptr frame(unsigned i) { return (dream_ptr)(uintptr_t)frames[i]; }
static unsigned frame_index(dream_ptr f) { return (unsigned)(((unsigned char *)(uintptr_t)f - frames[0]) / F_SLOTS); }

/* Deadlines repeat so equal keys must fire in arming order; cancelled frames never fire. */
static void timer_heap_order(unsigned count) {
    uint32_t state = 12345u + count;
    int64_t dues[5000];
    for (unsigned i = 0; i < count; i++) {
        state = state * 1103515245u + 12345u;
        dues[i] = (int64_t)(state >> 16) % 97;
        timer_push(frame(i), dues[i]);
    }
    for (unsigned i = 0; i < count; i += 3) { timer_cancel(frame(i)); }
    for (unsigned i = 0; i < count; i += 3) {
        assert(i32_at(frame(i), F_REMAINING)[0] == 0);
        timer_cancel(frame(i));
    }
    int64_t last_due = -1;
    unsigned last_index = 0, fired = 0;
    while (timer_peek()) {
        dream_ptr f = timer_pop();
        unsigned i = frame_index(f);
        assert(i % 3 != 0 && i32_at(f, F_REMAINING)[0] == 0);
        assert(dues[i] > last_due || (dues[i] == last_due && i > last_index));
        last_due = dues[i];
        last_index = i;
        fired++;
    }
    assert(fired == count - (count + 2) / 3);
    timer_finish();
}
static DREAM_THREAD_PROC(foreign_churn) {
    unsigned slot = (unsigned)(uintptr_t)arg;
    for (unsigned i = 0; i < 10000; i++) {
        Node *node = foreign_node_take();
        node->f = (dream_ptr)(uintptr_t)(slot + 1);
        if ((i & 7u) == 0) { sched_yield(); }
        assert(node->f == (dream_ptr)(uintptr_t)(slot + 1));
        foreign_node_put(node);
    }
    return 0;
}
static Node *handoff[4][513];
static DREAM_THREAD_PROC(foreign_producer) {
    unsigned slot = (unsigned)(uintptr_t)arg;
    for (unsigned i = 0; i < 513; i++) {
        Node *node = foreign_node_take();
        assert(!node->f && !node->next);
        node->f = (dream_ptr)(uintptr_t)(slot * 513 + i + 1);
        handoff[slot][i] = node;
    }
    return 0;
}
static void foreign_burst(void) {
    dream_thread threads[4];
    for (unsigned i = 0; i < 4; i++) {
        assert(dream_thread_start(&threads[i], foreign_producer, (void *)(uintptr_t)i) == 0);
    }
    for (unsigned i = 0; i < 4; i++) { dream_thread_join(threads[i]); }
    assert(foreign_pool.live == 4 * 513);
    for (unsigned i = 0; i < 513; i++) {
        for (unsigned slot = 0; slot < 4; slot++) {
            Node *node = handoff[slot][i];
            assert(node->f == (dream_ptr)(uintptr_t)(slot * 513 + i + 1));
            foreign_node_put(node);
        }
    }
    assert(!foreign_pool.live && !foreign_pool.blocks && !foreign_pool.free);
    assert(foreign_cache.count <= NODE_CACHED_BLOCKS);
}
int main(void) {
    unsigned before = allocations;
    timer_heap_order(0);
    timer_heap_order(1);
    timer_heap_order(TIMER_INLINE_CAPACITY);
    assert(allocations == before);
    unsigned sizes[] = {TIMER_INLINE_CAPACITY + 1, 65, 513, 4096, 5000};
    for (unsigned i = 0; i < sizeof(sizes) / sizeof(sizes[0]); i++) { timer_heap_order(sizes[i]); }
    assert(allocations == deallocations);
    dream_thread threads[4];
    foreign_burst();
    before = allocations;
    for (unsigned i = 0; i < 4; i++) {
        assert(dream_thread_start(&threads[i], foreign_churn, (void *)(uintptr_t)i) == 0);
    }
    for (unsigned i = 0; i < 4; i++) { dream_thread_join(threads[i]); }
    assert(allocations == before);
    for (unsigned i = 0; i < 100; i++) {
        Node *node = foreign_node_take();
        foreign_node_put(node);
    }
    assert(allocations == before);
    assert(allocations - deallocations <= NODE_CACHED_BLOCKS);
    node_blocks_free(foreign_cache.blocks);
    assert(allocations == deallocations);
    return 0;
}
