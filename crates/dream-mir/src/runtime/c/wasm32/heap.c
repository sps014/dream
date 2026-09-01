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

#define REGION_MAX_DEPTH 8
#define REGION_PAYLOAD (4 << 20)

static int region_owns(dream_ptr ptr) {
    int32_t p;
    int32_t b;
    int32_t cap;
    if (dream_region_depth_get() <= 0 || dream_region_slab_get() == 0) {
        return 0;
    }
    p = (int32_t)ptr;
    b = dream_region_slab_get();
    cap = dream_region_cap_get();
    return p >= b && p < b + cap;
}

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

static dream_ptr malloc_locked(int32_t size, int32_t tag);
static void recycle_locked(dream_ptr ptr);

#define PRIV_SLAB (2 << 20)

static void account_alloc(void) {
    __atomic_fetch_add(&live_objects, 1, __ATOMIC_RELAXED);
    __atomic_fetch_add(&total_allocations, 1, __ATOMIC_RELAXED);
}

static void account_free_n(int32_t n) {
    __atomic_fetch_add(&last_freed, n, __ATOMIC_RELAXED);
    __atomic_fetch_sub(&live_objects, n, __ATOMIC_RELAXED);
}

static int32_t claim_bytes(int32_t n) {
    int32_t start;
#ifdef DREAM_WASM32_THREADS
    start = __atomic_fetch_add(meta_i32(META_HEAP_PTR), n, __ATOMIC_RELAXED);
#else
    start = heap_ptr_get();
    heap_ptr_set(start + n);
#endif
    ensure_pages(start + n);
    return start;
}

static void priv_class_push(int32_t idx, int32_t block) {
    int32_t head = dream_priv_fl_get(idx);
    blk_set_next(block, head);
    dream_priv_fl_set(idx, block);
}

static dream_ptr finish_block(int32_t block, int32_t tag) {
    i32_put(block + (int32_t)HEADER_TAG_OFFSET, tag);
    i32_put(block + (int32_t)HEADER_REFCOUNT_OFFSET, 1);
    account_alloc();
    return (dream_ptr)(block + (int32_t)HEAP_HEADER_SIZE);
}

static void priv_refill(int32_t need) {
    int32_t n = need > PRIV_SLAB ? need : PRIV_SLAB;
    n = (n + 15) & -16;
    dream_priv_slab_set(claim_bytes(n));
    dream_priv_off_set(0);
    dream_priv_cap_set(n);
}

static dream_ptr malloc_private(int32_t size, int32_t tag) {
    int32_t total;
    int32_t idx;
    int32_t block;
    int32_t next;
    int32_t off;
    int32_t cap;
    int32_t slab;
    total = round_total(size);
    idx = size_class(total);
    if (idx <= 12) {
        block = dream_priv_fl_get(idx);
        if (block) {
            next = blk_next(block);
            dream_priv_fl_set(idx, next);
            return finish_block(block, tag);
        }
    }
    off = dream_priv_off_get();
    cap = dream_priv_cap_get();
    if (cap - off < total) {
        priv_refill(total);
        off = 0;
    }
    slab = dream_priv_slab_get();
    block = slab + off;
    dream_priv_off_set(off + total);
    i32_put(block, total);
    return finish_block(block, tag);
}

static int32_t *region_mark_off(void) {
    int32_t p = dream_region_marks_get();
    return (int32_t *)(uintptr_t)(uint32_t)p;
}

static int32_t *region_mark_nalloc(void) {
    return region_mark_off() + REGION_MAX_DEPTH;
}

static dream_ptr region_malloc_locked(int32_t size, int32_t tag) {
    int32_t block;
    int32_t total;
    int32_t off = dream_region_off_get();
    int32_t cap = dream_region_cap_get();
    total = round_total(size);
    if (off == 0) {
        /* Payload of the slab is 16-aligned; wasm blocks must start at 4 (mod 16). */
        off = 4;
    }
    if (total < 0 || off > cap - total) {
        abort();
    }
    block = dream_region_slab_get() + off;
    dream_region_off_set(off + total);
    i32_put(block, total);
    i32_put(block + (int32_t)HEADER_TAG_OFFSET, tag);
    i32_put(block + (int32_t)HEADER_REFCOUNT_OFFSET, 1);
    account_alloc();
    dream_region_nalloc_set(dream_region_nalloc_get() + 1);
    return (dream_ptr)(block + (int32_t)HEAP_HEADER_SIZE);
}

void dream_region_enter(void) {
    int32_t depth = dream_region_depth_get();
    int32_t *off_m;
    int32_t *n_m;
    if (depth >= REGION_MAX_DEPTH) {
        abort();
    }
    if (dream_region_marks_get() == 0) {
        dream_region_marks_set((int32_t)malloc_private(REGION_MAX_DEPTH * 8, 0));
    }
    if (depth == 0) {
        dream_region_slab_set((int32_t)malloc_private(REGION_PAYLOAD, 0));
        dream_region_cap_set(round_total(REGION_PAYLOAD) - (int32_t)HEAP_HEADER_SIZE);
        dream_region_off_set(0);
        dream_region_nalloc_set(0);
    }
    off_m = region_mark_off();
    n_m = region_mark_nalloc();
    off_m[depth] = dream_region_off_get();
    n_m[depth] = dream_region_nalloc_get();
    dream_region_depth_set(depth + 1);
}

void dream_region_leave(void) {
    int32_t n;
    int32_t depth = dream_region_depth_get();
    int32_t *off_m;
    int32_t *n_m;
    dream_ptr slab;
    if (depth <= 0) {
        return;
    }
    depth -= 1;
    dream_region_depth_set(depth);
    off_m = region_mark_off();
    n_m = region_mark_nalloc();
    n = dream_region_nalloc_get() - n_m[depth];
    if (n < 0) {
        n = 0;
    }
    account_free_n(n);
    dream_region_nalloc_set(n_m[depth]);
    dream_region_off_set(off_m[depth]);
    if (depth == 0) {
        slab = (dream_ptr)dream_region_slab_get();
        dream_region_slab_set(0);
        dream_region_cap_set(0);
        dream_region_off_set(0);
        dream_region_nalloc_set(0);
        if (slab) {
            dream_recycle(slab);
        }
    }
}

static dream_ptr malloc_locked(int32_t size, int32_t tag) {
    int32_t idx;
    int32_t *head;
    int32_t block = 0;
    int32_t next;

    if ((tag & TAG_SHARED) == 0 && dream_region_depth_get() > 0 && dream_region_slab_get() != 0) {
        return region_malloc_locked(size, tag);
    }

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
        block = claim_bytes(size);
        i32_put(block, size);
    } else if (i32_at(block) >= size + MIN_SPLIT) {
        /* Split a reused block much bigger than this request. */
        large_split(block, i32_at(block), size);
    }

    i32_put(block + (int32_t)HEADER_TAG_OFFSET, tag);
    i32_put(block + (int32_t)HEADER_REFCOUNT_OFFSET, 1);
    account_alloc();
    return (dream_ptr)(block + (int32_t)HEAP_HEADER_SIZE);
}

int32_t debug_get_live_objects(void) {
    return __atomic_load_n(&live_objects, __ATOMIC_RELAXED);
}
int32_t debug_get_total_allocations(void) {
    return __atomic_load_n(&total_allocations, __ATOMIC_RELAXED);
}
int32_t debug_get_ref_count(dream_ptr ptr) {
    return ptr ? ((int32_t *)((char *)dream_p(ptr) - RC_FROM_DATA))[0] : 0;
}
int32_t debug_get_heap_ptr(void) { return heap_ptr_get(); }
/* Native parity: the probe exposes "most recent freed block" (a free-happened detector),
 * not this allocator's internal list head, which coalescing keeps stable. */
int32_t debug_get_free_list_head(void) { return last_freed; }

__attribute__((export_name(DREAM_SYM_MALLOC)))
dream_ptr dream_malloc(int32_t size, int32_t tag) {
    if (dream_region_depth_get() > 0 && dream_region_slab_get() != 0) {
        return region_malloc_locked(size, tag);
    }
    return malloc_private(size, tag);
}

dream_ptr dream_malloc_shared(int32_t size, int32_t tag) {
    dream_ptr p;
    if (tag != 0) {
        tag |= TAG_SHARED;
    }
    alloc_lock();
    p = malloc_locked(size, tag);
    alloc_unlock();
    return p;
}

__attribute__((export_name("dream_publish")))
void dream_publish(dream_ptr ptr) {
    int32_t *tag;
    if (!ptr) {
        return;
    }
    tag = (int32_t *)((char *)dream_p(ptr) - TAG_FROM_DATA);
    if ((*tag & TAG_VALUE_MASK) == 0) {
        return;
    }
    *tag |= TAG_SHARED;
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

static void recycle_locked(dream_ptr ptr) {
    int32_t block_start;
    int32_t idx;
    int32_t sz;
    if (region_owns(ptr)) {
        return;
    }
    block_start = (int32_t)ptr - (int32_t)HEAP_HEADER_SIZE;
    sz = i32_at(block_start);
    if (sz == 0) {
        return;
    }
    account_free_n(1);
    free_list_head = block_start;
    idx = size_class(sz);
    if (idx > 12 || sz != class_bytes(idx)) {
        free_large_locked(block_start, sz);
    } else {
        class_push(idx, block_start);
    }
}

void dream_recycle(dream_ptr ptr) {
    int32_t block_start;
    int32_t idx;
    int32_t sz;
    if (!ptr) {
        return;
    }
    if (dream_weak_any) {
        dream_weak_clear_all(ptr);
    }
    if (region_owns(ptr)) {
        return;
    }
    if (dream_tag_shared(ptr)) {
        alloc_lock();
        recycle_locked(ptr);
        alloc_unlock();
        return;
    }
    block_start = (int32_t)ptr - (int32_t)HEAP_HEADER_SIZE;
    sz = i32_at(block_start);
    if (sz == 0) {
        return;
    }
    account_free_n(1);
    idx = size_class(sz);
    if (idx <= 12 && sz == class_bytes(idx)) {
        priv_class_push(idx, block_start);
        return;
    }
    alloc_lock();
    free_large_locked(block_start, sz);
    alloc_unlock();
}

__attribute__((export_name(DREAM_SYM_FREE)))
void dream_free(dream_ptr ptr) {
    if (!ptr) {
        return;
    }
    /* Substring slices retain their parent; release it before the block leaves the live
     * set. Weak slots pointing at this object are reset first so `del`-time observers see
     * the cleared state (mirrors native/heap.c). */
    dream_str_fini(ptr);
    dream_recycle(ptr);
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
    np = dream_tag_shared(ptr) ? dream_malloc_shared(new_size, tag) : dream_malloc(new_size, tag);
    copy = old_total - (int32_t)HEAP_HEADER_SIZE;
    if (copy > new_size) {
        copy = new_size;
    }
    memcpy(dream_p(np), dream_p(ptr), (size_t)copy);
    dream_release(ptr);
    return np;
}
