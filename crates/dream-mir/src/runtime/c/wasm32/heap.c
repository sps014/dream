#include "dream_rt_wasm32.h"

#include <limits.h>

#ifdef DREAM_WASM32_THREADS
int dream_rt_mt = 1;
#else
int dream_rt_mt;
#endif
int32_t live_objects;
int32_t total_allocations;
int32_t last_freed;
int32_t free_list_head;

#ifdef __wasm__
extern unsigned char __heap_base;

static inline int32_t wasm_memory_size(void) {
    return (int32_t)__builtin_wasm_memory_size(0);
}

static inline int32_t wasm_memory_grow(int32_t delta) {
    return (int32_t)__builtin_wasm_memory_grow(0, (size_t)delta);
}
#endif

/* Runtime meta lives at `__heap_base`, not `.bss`. Worker instantiate re-applies
 * active data/bss segments onto shared memory and would reset bump/tid/freelists. */
enum {
    META_HEAP_PTR = 0,
    META_TID = 4,
    META_LOCK = 8,
    META_FL = 12,
    META_SIZE = 80
};

static int32_t heap_start(void) {
#ifdef __wasm__
    int32_t b = (int32_t)(uintptr_t)&__heap_base;
    return (b + 15) & ~15;
#else
    return (int32_t)STRING_BASE;
#endif
}

static int32_t meta_base(void) { return heap_start(); }

static int32_t *meta_i32(int32_t off) {
    return (int32_t *)(uintptr_t)(uint32_t)(meta_base() + off);
}

static int32_t heap_ptr_get(void) { return *meta_i32(META_HEAP_PTR); }

static void heap_ptr_set(int32_t v) { *meta_i32(META_HEAP_PTR) = v; }

static int32_t i32_at(int32_t addr) {
    return *(int32_t *)(uintptr_t)(uint32_t)addr;
}

static void i32_put(int32_t addr, int32_t v) {
    *(int32_t *)(uintptr_t)(uint32_t)addr = v;
}

static int32_t size_class(int32_t size) {
    int32_t s = size;
    if (s < 16) {
        s = 16;
    }
    return 28 - __builtin_clz((unsigned)(s - 1));
}

static int32_t *class_head(int32_t idx) {
    int32_t i = idx > 12 ? 13 : idx;
    return meta_i32(META_FL + i * 4);
}

static int32_t class_bytes(int32_t idx) {
    if (idx > 12) {
        return 0;
    }
    return 1 << (idx + 4);
}

#ifdef DREAM_WASM32_THREADS
static void alloc_lock(void) {
    for (;;) {
        int32_t expected = 0;
        if (__atomic_compare_exchange_n(meta_i32(META_LOCK), &expected, 1, 0, __ATOMIC_ACQUIRE,
                                        __ATOMIC_RELAXED)) {
            return;
        }
    }
}

static void alloc_unlock(void) {
    __atomic_store_n(meta_i32(META_LOCK), 0, __ATOMIC_RELEASE);
}
#else
static void alloc_lock(void) {}
static void alloc_unlock(void) {}
#endif

void dream_heap_init(void) {
    int32_t desired = heap_start() + META_SIZE;
#ifdef DREAM_WASM32_THREADS
    int32_t expected = 0;
    (void)__atomic_compare_exchange_n(meta_i32(META_HEAP_PTR), &expected, desired, 0,
                                      __ATOMIC_RELAXED, __ATOMIC_RELAXED);
#else
    if (heap_ptr_get() == 0) {
        heap_ptr_set(desired);
    }
#endif
}

int32_t dream_next_tid(void) {
    return __atomic_fetch_add(meta_i32(META_TID), 1, __ATOMIC_RELAXED) + 1;
}

static void ensure_pages(int32_t new_heap) {
#ifdef __wasm__
    int32_t cur;
    int32_t need;
    int32_t delta;
    cur = wasm_memory_size() << 16;
    if (new_heap <= cur) {
        return;
    }
    need = ((new_heap - 1) >> 16) + 1;
    delta = need - wasm_memory_size();
    if (wasm_memory_grow(delta) == -1) {
        __builtin_trap();
    }
#else
    (void)new_heap;
#endif
}

static dream_ptr malloc_locked(int32_t size, int32_t tag) {
    int32_t idx;
    int32_t alloc_size;
    int32_t *head;
    int32_t block = 0;
    int32_t next;
    int32_t curr;
    int32_t prev;
    int32_t block_size;
    int32_t new_heap;

    size = ((size + 3) & -4) + (int32_t)HEAP_HEADER_SIZE;
    idx = size_class(size);
    head = class_head(idx);
    if (idx > 12) {
        alloc_size = size;
        curr = *head;
        prev = 0;
        while (curr) {
            block_size = i32_at(curr);
            if (block_size >= alloc_size) {
                next = i32_at(curr + (int32_t)HEADER_TAG_OFFSET);
                if (prev) {
                    i32_put(prev + (int32_t)HEADER_TAG_OFFSET, next);
                } else {
                    *head = next;
                }
                block = curr;
                break;
            }
            prev = curr;
            curr = i32_at(curr + (int32_t)HEADER_TAG_OFFSET);
        }
    } else {
        alloc_size = class_bytes(idx);
        block = *head;
        if (block) {
            next = i32_at(block + (int32_t)HEADER_TAG_OFFSET);
            *head = next;
        }
    }

    if (!block) {
        block = heap_ptr_get();
        new_heap = block + alloc_size;
        ensure_pages(new_heap);
        heap_ptr_set(new_heap);
        i32_put(block, alloc_size);
    }

    i32_put(block + (int32_t)HEADER_TAG_OFFSET, tag);
    i32_put(block + (int32_t)HEADER_REFCOUNT_OFFSET, 1);
    live_objects += 1;
    total_allocations += 1;
    return (dream_ptr)(block + (int32_t)HEAP_HEADER_SIZE);
}

int32_t debug_get_live_objects(void) { return live_objects; }
int32_t debug_get_total_allocations(void) { return total_allocations; }
int32_t debug_get_ref_count(dream_ptr ptr) {
    return ptr ? ((int32_t *)((char *)dream_p(ptr) - RC_FROM_DATA))[0] : 0;
}
int32_t debug_get_heap_ptr(void) { return heap_ptr_get(); }
int32_t debug_get_free_list_head(void) { return free_list_head; }

__attribute__((export_name(DREAM_SYM_MALLOC)))
dream_ptr dream_malloc(int32_t size, int32_t tag) {
    dream_ptr p;
    alloc_lock();
    p = malloc_locked(size, tag);
    alloc_unlock();
    return p;
}

__attribute__((export_name(DREAM_SYM_FREE)))
void dream_free(dream_ptr ptr) {
    int32_t block_start;
    int32_t idx;
    int32_t *head;
    int32_t sz;
    if (!ptr) {
        return;
    }
    alloc_lock();
    block_start = (int32_t)ptr - (int32_t)HEAP_HEADER_SIZE;
    sz = i32_at(block_start);
    if (sz == 0) {
        alloc_unlock();
        return;
    }
    live_objects -= 1;
    last_freed = block_start;
    idx = size_class(sz);
    head = class_head(idx);
    i32_put(block_start + (int32_t)HEADER_TAG_OFFSET, *head);
    *head = block_start;
    free_list_head = block_start;
    alloc_unlock();
}

dream_ptr dream_realloc(dream_ptr ptr, int32_t new_size, int32_t tag) {
    int32_t block_start;
    int32_t old_total;
    int32_t new_total;
    dream_ptr np;
    int32_t copy;
    if (!ptr) {
        return dream_malloc(new_size, tag);
    }
    block_start = (int32_t)ptr - (int32_t)HEAP_HEADER_SIZE;
    old_total = i32_at(block_start);
    new_total = ((new_size + 3) & -4) + (int32_t)HEAP_HEADER_SIZE;
    if (new_total <= old_total) {
        return ptr;
    }
    np = dream_malloc(new_size, tag);
    copy = old_total - (int32_t)HEAP_HEADER_SIZE;
    if (copy > new_size) {
        copy = new_size;
    }
    memcpy(dream_p(np), dream_p(ptr), (size_t)copy);
    dream_free(ptr);
    return np;
}
