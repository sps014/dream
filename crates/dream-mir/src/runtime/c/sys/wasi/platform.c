#include "dream_heap_internal.h"
#include "dream_platform_internal.h"
#include <stdlib.h>
static void platform_lock(unsigned domain) {
#ifdef DREAM_WASM32_THREADS
    int32_t *word = dream_wasm32_meta_i32(domain == DREAM_LOCK_WEAK ? META_WEAK_LOCK : META_LOCK);
    for (;;) {
        int32_t expected = 0;
        if (__atomic_compare_exchange_n(word, &expected, 1, 0, __ATOMIC_ACQUIRE, __ATOMIC_RELAXED)) { return; }
    }
#else
    (void)domain;
#endif
}
static void platform_unlock(unsigned domain) {
#ifdef DREAM_WASM32_THREADS
    int32_t *word = dream_wasm32_meta_i32(domain == DREAM_LOCK_WEAK ? META_WEAK_LOCK : META_LOCK);
    __atomic_store_n(word, 0, __ATOMIC_RELEASE);
#else
    (void)domain;
#endif
}
static void platform_abort(void) { __builtin_trap(); }
static void *unused_map(size_t size) { (void)size; return NULL; }
static void *platform_resize(void *p, size_t size) {
    if (size > INT32_MAX) { return NULL; }
    return realloc(p, size);
}
static void platform_object_drop(void *ptr) { (void)ptr; }
const dream_platform dream_default_platform = {
    malloc, platform_resize, free, unused_map, platform_abort, platform_write, platform_lock, platform_unlock, platform_object_drop
};
