#include "dream_core.h"
#include "dream_thread.h"
#include "dream_platform_internal.h"
#include <assert.h>
#include <stdlib.h>
#include <sched.h>
#include "../crates/dream-mir/src/runtime/c/sys/shared/node_pool.h"

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

static void timer_burst(unsigned count) {
    Node *nodes[4096];
    assert(count <= 4096);
    for (unsigned i = 0; i < count; i++) {
        nodes[i] = timer_node_take();
        assert(!nodes[i]->f && !nodes[i]->next && !nodes[i]->due);
        nodes[i]->f = (dream_ptr)(uintptr_t)(i + 1);
        nodes[i]->due = i;
    }
    /* Retirement of one entry cannot invalidate another entry still active in its block. */
    for (unsigned i = count; i > 0; i--) {
        assert(nodes[i - 1]->f == (dream_ptr)(uintptr_t)i && nodes[i - 1]->due == i - 1);
        timer_node_put(nodes[i - 1]);
    }
    assert(!timer_pool.live && !timer_pool.free && !timer_pool.blocks);
}
static DREAM_THREAD_PROC(timer_worker) {
    (void)arg;
    for (unsigned i = 0; i < 100; i++) { timer_burst(256); }
    return 0;
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
        assert(!node->f && !node->next && !node->due);
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
    unsigned sizes[] = {0, 1, 63, 64, 65, 127, 128, 256, 513, 4096};
    for (unsigned i = 0; i < sizeof(sizes) / sizeof(sizes[0]); i++) { timer_burst(sizes[i]); }
    timer_burst(256);
    unsigned before = allocations;
    for (unsigned i = 0; i < 100; i++) { timer_burst(256); }
    assert(allocations == before);
    dream_thread threads[4];
    for (unsigned i = 0; i < 4; i++) { assert(dream_thread_start(&threads[i], timer_worker, NULL) == 0); }
    for (unsigned i = 0; i < 4; i++) { dream_thread_join(threads[i]); }
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
    assert(allocations - deallocations <= 2 * NODE_CACHED_BLOCKS);
    node_blocks_free(timer_cache.blocks);
    node_blocks_free(foreign_cache.blocks);
    assert(allocations == deallocations);
    return 0;
}
