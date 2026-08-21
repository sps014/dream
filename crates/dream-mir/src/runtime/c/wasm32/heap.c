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
    META_WEAK_LOCK = 72,
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

int32_t *dream_wasm32_meta_i32(int32_t off) { return meta_i32(off); }

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

/* Blocks > LARGE_MAX bytes live on one address-ordered free list (slot 13) so physical
 * neighbors can be found and merged on free; smaller blocks use exact-fit per-class LIFO
 * lists (slots 0..12). Requests whose class list is empty are served by splitting a large
 * free block, so cross-class churn does not strand memory. */
enum { MIN_SPLIT = 32 };

static int32_t *large_head(void) {
    return class_head(13);
}

static int32_t blk_next(int32_t block) {
    return i32_at(block + (int32_t)HEADER_TAG_OFFSET);
}

static void blk_set_next(int32_t block, int32_t next) {
    i32_put(block + (int32_t)HEADER_TAG_OFFSET, next);
}

static void class_push(int32_t idx, int32_t block) {
    int32_t *head = class_head(idx);
    blk_set_next(block, *head);
    *head = block;
}

/* Insert into the address-ordered large-free list. */
static void large_insert(int32_t block) {
    int32_t *head = large_head();
    int32_t curr = *head;
    int32_t prev = 0;
    while (curr && curr < block) {
        prev = curr;
        curr = blk_next(curr);
    }
    blk_set_next(block, curr);
    if (prev) {
        blk_set_next(prev, block);
    } else {
        *head = block;
    }
}

static void large_remove(int32_t block) {
    int32_t *head = large_head();
    int32_t curr = *head;
    int32_t prev = 0;
    while (curr && curr != block) {
        prev = curr;
        curr = blk_next(curr);
    }
    if (!curr) {
        return;
    }
    if (prev) {
        blk_set_next(prev, blk_next(block));
    } else {
        *head = blk_next(block);
    }
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
    /* Blocks must start at 4 (mod 16) so the payload at block+12 is 8-aligned, and block
     * totals must be multiples of 16 (see round_total) so every successor block — bump,
     * split remainder, or merge — keeps that residue. */
    int32_t desired = (heap_start() + META_SIZE + 15) & -16;
    desired += 4;
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

/* Total block size for `payload` bytes: 8-aligned payload + 12-byte header, padded to a
 * multiple of 16 so block starts stay at 4 (mod 16) across bump/split/merge. */
static int32_t round_total(int32_t payload) {
    int32_t t = ((payload + 7) & -8) + (int32_t)HEAP_HEADER_SIZE;
    return (t + 15) & -16;
}

/* Carve `need` bytes off the front of the free block at `block` (size at [block]).
 * The remainder, when big enough to be a block of its own, goes back on the right list. */
static void free_insert(int32_t block, int32_t sz);

static void large_split(int32_t block, int32_t bsize, int32_t need) {
    int32_t rem;
    if (bsize - need < MIN_SPLIT) {
        return;
    }
    rem = block + need;
    i32_put(block, need);
    i32_put(rem, bsize - need);
    free_insert(rem, bsize - need);
}

/* First fit over the address-ordered large-free list; splits the chosen block. */
static int32_t take_from_large(int32_t need) {
    int32_t curr = *large_head();
    int32_t guard = 0;
    while (curr) {
        int32_t bsize;
        if (++guard > 1000000) {
            __builtin_trap();
        }
        bsize = i32_at(curr);
        if (bsize >= need) {
            large_remove(curr);
            large_split(curr, bsize, need);
            return curr;
        }
        curr = blk_next(curr);
    }
    return 0;
}

/* Return a free block to the right list. Class lists hold ONLY exact-class-size blocks
 * (so a pop always satisfies its class's largest request); anything else — split
 * remainders, exact-size-rounded frees — goes on the size-checked large list. */
static void free_insert(int32_t block, int32_t sz) {
    int32_t idx = size_class(sz);
    if (idx <= 12 && sz == class_bytes(idx)) {
        class_push(idx, block);
    } else {
        large_insert(block);
    }
}

static dream_ptr malloc_locked(int32_t size, int32_t tag) {
    int32_t idx;
    int32_t *head;
    int32_t block = 0;
    int32_t next;
    int32_t new_heap;

    size = round_total(size);
    idx = size_class(size);
    head = class_head(idx);
    if (idx > 12) {
        block = take_from_large(size);
    } else {
        block = *head;
        if (block) {
            next = blk_next(block);
            *head = next;
        } else {
            block = take_from_large(size);
        }
    }

    if (!block) {
        block = heap_ptr_get();
        new_heap = block + size;
        ensure_pages(new_heap);
        heap_ptr_set(new_heap);
        i32_put(block, size);
    } else if (i32_at(block) >= size + MIN_SPLIT) {
        /* Split a reused block much bigger than this request. */
        large_split(block, i32_at(block), size);
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
/* Native parity: the probe exposes "most recent freed block" (a free-happened detector),
 * not this allocator's internal list head, which coalescing keeps stable. */
int32_t debug_get_free_list_head(void) { return last_freed; }

__attribute__((export_name(DREAM_SYM_MALLOC)))
dream_ptr dream_malloc(int32_t size, int32_t tag) {
    dream_ptr p;
    alloc_lock();
    p = malloc_locked(size, tag);
    alloc_unlock();
    return p;
}

/* Free a large block, merging with physically adjacent free neighbors (the large list is
 * address-ordered, so both directions are one walk). Small free blocks in between block
 * merging across them — an accepted approximation to keep frees O(large-list length). */
static void free_large_locked(int32_t block, int32_t sz) {
    int32_t curr = *large_head();
    int32_t prev = 0;
    int32_t next_phys = block + sz;
    while (curr && curr < block) {
        prev = curr;
        curr = blk_next(curr);
    }
    /* Merge the following neighbor first. */
    if (curr == next_phys) {
        sz += i32_at(curr);
        curr = blk_next(curr);
    }
    /* Then merge into the preceding neighbor when it ends exactly at us. The forward
     * neighbor (already folded into `sz`) must be unlinked here too — it sits between
     * `prev` and `curr` in the list, and its memory now belongs to the merged block. */
    if (prev && prev + i32_at(prev) == block) {
        i32_put(prev, i32_at(prev) + sz);
        blk_set_next(prev, curr);
        return;
    }
    if (prev) {
        blk_set_next(prev, block);
    } else {
        *large_head() = block;
    }
    blk_set_next(block, curr);
    i32_put(block, sz);
}

__attribute__((export_name(DREAM_SYM_FREE)))
void dream_free(dream_ptr ptr) {
    int32_t block_start;
    int32_t idx;
    int32_t sz;
    if (!ptr) {
        return;
    }
    /* Substring slices retain their parent; release it before the block leaves the live
     * set. Weak slots pointing at this object are reset first so `del`-time observers see
     * the cleared state (mirrors native/heap.c). */
    dream_str_fini(ptr);
    if (dream_weak_any) {
        dream_weak_clear_all(ptr);
    }
    alloc_lock();
    block_start = (int32_t)ptr - (int32_t)HEAP_HEADER_SIZE;
    sz = i32_at(block_start);
    if (sz == 0) {
        alloc_unlock();
        return;
    }
    live_objects -= 1;
    last_freed += 1;
    free_list_head = block_start;
    idx = size_class(sz);
    if (idx > 12 || sz != class_bytes(idx)) {
        free_large_locked(block_start, sz);
    } else {
        class_push(idx, block_start);
    }
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
    new_total = round_total(new_size);
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
