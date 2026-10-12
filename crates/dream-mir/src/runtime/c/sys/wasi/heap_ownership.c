#include "dream_heap_internal.h"

#include <limits.h>

/* `Debug.live_objects` is derived on read, so each allocation and free bumps one counter. */
int64_t total_allocations;
int64_t total_frees;
int64_t pinned_objects;
uint64_t dream_weak_allocations;
uint64_t dream_weak_frees;
int32_t last_freed;
int32_t free_list_head;

int64_t debug_get_live_objects(void) {
    uint64_t allocations = __atomic_load_n(&dream_weak_allocations, __ATOMIC_RELAXED);
    uint64_t frees = __atomic_load_n(&dream_weak_frees, __ATOMIC_RELAXED);
    int64_t live = __atomic_load_n(&total_allocations, __ATOMIC_RELAXED)
        - __atomic_load_n(&total_frees, __ATOMIC_RELAXED)
        - __atomic_load_n(&pinned_objects, __ATOMIC_RELAXED);
    return (live > 0 ? live : 0) + (int64_t)(allocations > frees ? allocations - frees : 0);
}
int64_t debug_get_total_allocations(void) {
    return __atomic_load_n(&total_allocations, __ATOMIC_RELAXED)
        + (int64_t)__atomic_load_n(&dream_weak_allocations, __ATOMIC_RELAXED);
}
int32_t debug_get_ref_count(dream_ptr ptr) {
    return ptr ? dream_rc_count(ptr) : 0;
}

void dream_pin_immortal(dream_ptr s) {
    if (!s) { return; }
    if (__atomic_exchange_n(dream_rc_word(s), DREAM_RC_IMMORTAL, __ATOMIC_RELAXED) != DREAM_RC_IMMORTAL) {
        __atomic_fetch_add(&pinned_objects, 1, __ATOMIC_RELAXED);
    }
}

__attribute__((cold, noinline)) void dream_retain_slow(int32_t *rc, int32_t v) {
    for (;;) {
        if (v == DREAM_RC_IMMORTAL) { return; }
        if (v == 0) { DREAM_PANIC_LITERAL(u"panic: resurrection of a dying object"); }
        if ((v & INT32_MAX) == INT32_MAX) { DREAM_PANIC_LITERAL(u"panic: reference count overflow"); }
        int32_t next = (int32_t)((uint32_t)v + 1u);
        if (__atomic_compare_exchange_n(rc, &v, next, 0, __ATOMIC_RELAXED, __ATOMIC_RELAXED)) { return; }
    }
}

/* Replicating the shared-count retry loop at every local-count release bloats hot functions. */
__attribute__((noinline)) int dream_rc_last_slow(int32_t *rc, int32_t v) {
    while (v != 0 && v != DREAM_RC_IMMORTAL) {
        /* Shared count zero has the immortal encoding. Never expose that intermediate
         * value: a weak loader could mistake it for a permanent owner and revive it. */
        int last = (v & INT32_MAX) == 1;
        int32_t next = last ? 0 : (int32_t)((uint32_t)v - 1u);
        if (__atomic_compare_exchange_n(rc, &v, next, 0, __ATOMIC_ACQ_REL, __ATOMIC_RELAXED)) {
            return last;
        }
    }
    if (v == 0) { DREAM_PANIC_LITERAL(u"panic: reference count underflow"); }
    return 0;
}

__attribute__((noinline)) int dream_release_nonlast_slow(int32_t *rc, int32_t v) {
    while (v != DREAM_RC_IMMORTAL && (v & INT32_MAX) > 1) {
        int32_t next = (int32_t)((uint32_t)v - 1u);
        if (__atomic_compare_exchange_n(rc, &v, next, 0, __ATOMIC_ACQ_REL, __ATOMIC_RELAXED)) { return 1; }
    }
    return v == DREAM_RC_IMMORTAL;
}
int32_t debug_get_heap_ptr(void) { return dream_wasm_heap_ptr_get(); }
/* Native parity: the probe exposes "most recent freed block" (a free-happened detector),
 * not this allocator's internal list head, which coalescing keeps stable. */
int32_t debug_get_free_list_head(void) { return last_freed; }

int dream_heap_is_live(dream_ptr ptr) {
    uint32_t address = (uint32_t)ptr;
    uint32_t start = (uint32_t)dream_wasm_heap_start() + META_SIZE;
    uint32_t end = (uint32_t)dream_wasm_heap_ptr_get();
    if ((address & 3) != 0 || address < HEAP_HEADER_SIZE) {
        return 0;
    }
    uint32_t block = address - HEAP_HEADER_SIZE;
    if (block < start || address > end) {
        return 0;
    }
    int32_t size = i32_at((int32_t)block);
    if (size < (int32_t)HEAP_HEADER_SIZE || (uint32_t)size > end - block) {
        return 0;
    }
    int32_t rc = __atomic_load_n(dream_rc_word(ptr), __ATOMIC_RELAXED);
    /* A claimed object remains readable until finalization completes. Reclaimed
     * blocks clear both words, including raw frees that bypass ARC. */
    return rc != 0 || dream_object_info(ptr) != NULL;
}

__attribute__((export_name(DREAM_SYM_FREE)))
void dream_free(dream_ptr ptr) {
    if (!ptr) {
        return;
    }
    /* Substring slices retain their parent; release it before the block leaves the live
     * set. Weak slots pointing at this object are reset first so `del`-time observers see
     * the cleared state (mirrors core/heap.c). */
    dream_weak_prepare_destroy(ptr);
    dream_str_fini(ptr);
    if (dream_object_tag(ptr) == TAG_FUTURE) {
        dream_future_fini(ptr);
    }
    dream_recycle(ptr);
}

