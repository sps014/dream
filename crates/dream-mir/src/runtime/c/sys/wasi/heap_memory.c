#include "dream_heap_internal.h"

#ifdef __wasm__
extern unsigned char __heap_base;

static inline int32_t wasm_memory_size(void) {
    return (int32_t)__builtin_wasm_memory_size(0);
}

static inline int32_t wasm_memory_grow(int32_t delta) {
    return (int32_t)__builtin_wasm_memory_grow(0, (size_t)delta);
}
#endif

int32_t dream_wasm_heap_start(void) {
#ifdef __wasm__
    int32_t b = (int32_t)(uintptr_t)&__heap_base;
    return (b + 15) & ~15;
#else
    return (int32_t)STRING_BASE;
#endif
}

static int32_t meta_base(void) { return dream_wasm_heap_start(); }

static int32_t *meta_i32(int32_t off) {
    /* Worker instantiation re-applies data segments, so shared metadata must live beyond them. */
    return (int32_t *)(uintptr_t)(uint32_t)(meta_base() + off);
}

int32_t dream_wasm_heap_ptr_get(void) { return *meta_i32(META_HEAP_PTR); }

static void heap_ptr_set(int32_t v) { *meta_i32(META_HEAP_PTR) = v; }

int32_t *dream_wasm32_meta_i32(int32_t off) { return meta_i32(off); }

void dream_heap_init(void) {
    /* Headers and block sizes preserve 16-byte payload alignment. */
    int32_t desired = (dream_wasm_heap_start() + META_SIZE + 15) & -16;
#ifdef DREAM_WASM32_THREADS
    int32_t expected = 0;
    (void)__atomic_compare_exchange_n(meta_i32(META_HEAP_PTR), &expected, desired, 0,
                                      __ATOMIC_RELAXED, __ATOMIC_RELAXED);
#else
    if (dream_wasm_heap_ptr_get() == 0) {
        heap_ptr_set(desired);
    }
#endif
}

int32_t dream_next_tid(void) {
    return __atomic_fetch_add(meta_i32(META_TID), 1, __ATOMIC_RELAXED) + 1;
}

static void ensure_pages(int32_t new_heap) {
#ifdef __wasm__
    uint32_t cur;
    int32_t need;
    int32_t delta;
    cur = (uint32_t)wasm_memory_size() << 16;
    if ((uint32_t)new_heap <= cur) {
        return;
    }
    need = ((new_heap - 1) >> 16) + 1;
    delta = need - wasm_memory_size();
    if (wasm_memory_grow(delta) == -1) {
        DREAM_PANIC_LITERAL(u"panic: out of memory growing the WASI heap");
    }
#else
    (void)new_heap;
#endif
}

int32_t dream_wasm_heap_claim(int32_t n) {
    int32_t start;
#ifdef DREAM_WASM32_THREADS
    start = __atomic_load_n(meta_i32(META_HEAP_PTR), __ATOMIC_RELAXED);
    for (;;) {
        if (n <= 0 || start < 0 || n > INT32_MAX - start) {
            DREAM_PANIC_LITERAL(u"panic: allocation size exceeds the WASI heap limit");
        }
        int32_t end = start + n;
        if (__atomic_compare_exchange_n(meta_i32(META_HEAP_PTR), &start, end, 0,
                                       __ATOMIC_RELAXED, __ATOMIC_RELAXED)) { break; }
    }
#else
    start = dream_wasm_heap_ptr_get();
    if (n <= 0 || start < 0 || n > INT32_MAX - start) {
        DREAM_PANIC_LITERAL(u"panic: allocation size exceeds the WASI heap limit");
    }
    heap_ptr_set(start + n);
#endif
    ensure_pages(start + n);
    return start;
}
