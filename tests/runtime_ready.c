#include "dream_core.h"
#include "dream_thread.h"
#include "dream_platform_internal.h"
#include <assert.h>
#include <stdlib.h>
#include "../crates/dream-mir/src/runtime/c/sys/shared/ready_queue.h"

static dream_mutex mutex = DREAM_MUTEX_INIT;
static unsigned allocations, deallocations;
static void *allocate(size_t size) {
    __atomic_fetch_add(&allocations, 1, __ATOMIC_RELAXED);
    return malloc(size);
}
static void deallocate(void *p) {
    __atomic_fetch_add(&deallocations, 1, __ATOMIC_RELAXED);
    free(p);
}
static void lock(unsigned domain) { assert(domain == DREAM_LOCK_HEAP); dream_mutex_lock(&mutex); }
static void unlock(unsigned domain) { assert(domain == DREAM_LOCK_HEAP); dream_mutex_unlock(&mutex); }
static const dream_platform platform = {.allocate = allocate, .deallocate = deallocate, .lock = lock, .unlock = unlock};
const dream_platform *dream_platform_current = &platform;
void dream_panic(dream_ptr message) { (void)message; abort(); }

static void burst(unsigned count) {
    for (unsigned i = 0; i < count; i++) {
        ReadyNode *node = ready_take((dream_ptr)(uintptr_t)(i + 1));
        if (rq_tail) { rq_tail->next = node; } else { rq_head = node; }
        rq_tail = node;
    }
    for (unsigned i = 0; i < count; i++) {
        ReadyNode *node = rq_head;
        assert(node && node->f == (dream_ptr)(uintptr_t)(i + 1));
        rq_head = node->next;
        ready_put(node);
    }
    rq_tail = NULL;
    assert(!rq_head);
    ready_finish();
    assert(!ready_blocks && !ready_free);
}
static DREAM_THREAD_PROC(worker) {
    (void)arg;
    for (unsigned i = 0; i < 100; i++) { burst(513); }
    return 0;
}
int main(void) {
    const unsigned sizes[] = {0, 1, 63, 64, 65, 127, 128, 513, 1024, 4096};
    for (unsigned i = 0; i < sizeof(sizes) / sizeof(sizes[0]); i++) { burst(sizes[i]); }
    burst(513);
    unsigned before = allocations;
    for (unsigned i = 0; i < 100; i++) { burst(513); }
    assert(allocations == before);
    dream_thread threads[4];
    for (unsigned i = 0; i < 4; i++) { assert(dream_thread_start(&threads[i], worker, NULL) == 0); }
    for (unsigned i = 0; i < 4; i++) { dream_thread_join(threads[i]); }
    assert(allocations - deallocations <= READY_CACHED_BLOCKS);
    while (ready_cached) {
        ReadyBlock *block = ready_cached;
        ready_cached = block->next;
        deallocate(block);
    }
    assert(allocations == deallocations);
    return 0;
}
