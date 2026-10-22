#include "dream_heap_internal.h"
#include "dream_platform_internal.h"
#include "dream_region.h"

#include <limits.h>

#ifdef DREAM_WASM32_THREADS
int dream_rt_mt = 1;
#else
int dream_rt_mt;
#endif
static int32_t *class_head(int32_t idx) {
    int32_t i = idx > 12 ? 13 : idx;
    return dream_wasm32_meta_i32(META_FL + i * 4);
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
    return dream_i32(block + (int32_t)HEADER_TAG_OFFSET)[0];
}

static void blk_set_next(int32_t block, int32_t next) {
    dream_i32(block + (int32_t)HEADER_TAG_OFFSET)[0] = next;
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

/* Small allocations must fill their size class: non-class blocks return to the
 * address-ordered large list, making collector scratch allocation churn quadratic. */
static int32_t round_total(int32_t payload) {
    if (payload < 0 || payload > INT32_MAX - (int32_t)HEAP_HEADER_SIZE - 31) {
        DREAM_PANIC_LITERAL(u"panic: allocation size exceeds the WASI heap limit");
    }
    int32_t t = ((payload + 7) & -8) + (int32_t)HEAP_HEADER_SIZE;
    t = (t + 15) & -16;
    int32_t idx = size_class(t);
    return idx <= 12 ? class_bytes(idx) : t;
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
    dream_i32(block)[0] = need;
    dream_i32(rem)[0] = bsize - need;
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
        bsize = dream_i32(curr)[0];
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

static dream_ptr malloc_locked(int32_t size, int32_t tag, int account);
static void recycle_locked(dream_ptr ptr, int account);

#define PRIV_SLAB (2 << 20)

static void account_alloc(void) {
#if DREAM_HEAP_COUNTING
    __atomic_fetch_add(&total_allocations, 1, __ATOMIC_RELAXED);
#endif
}

static void account_free_n(int32_t n) {
#if DREAM_HEAP_COUNTING
    __atomic_fetch_add(&last_freed, n, __ATOMIC_RELAXED);
    if (n > 0) {
        __atomic_fetch_add(&total_frees, n, __ATOMIC_RELAXED);
    }
#else
    (void)n;
#endif
}

/* Per-instance exact-class freelists live in linear memory, indexed by class. `fast` aliases
 * the table except while a region is open (and before first use), so the inline paths below
 * fall through to the out-of-line ones with one test. */
enum { PRIV_CLASSES = 13, PRIV_SMALL_TOTAL = 1 << 16 };

static int32_t *priv_table(void) {
    int32_t table = dream_priv_table_get();
    if (table == 0) {
        table = dream_wasm_raw_malloc(PRIV_CLASSES * 4);
        memset((void *)(uintptr_t)(uint32_t)table, 0, PRIV_CLASSES * 4);
        dream_priv_table_set(table);
        if (!dream_region_active()) {
            dream_priv_fast_set(table);
        }
    }
    return (int32_t *)(uintptr_t)(uint32_t)table;
}

static void priv_class_push(int32_t idx, int32_t block) {
    int32_t *table = priv_table();
    blk_set_next(block, table[idx]);
    table[idx] = block;
}

DREAM_ALWAYS_INLINE dream_ptr activate_block(int32_t block, int32_t tag, const dream_type_info *info, int32_t account) {
    dream_i32(block + (int32_t)HEADER_TAG_OFFSET)[0] = tag;
    dream_i32(block + (int32_t)HEADER_REFCOUNT_OFFSET)[0] = dream_rc_init(tag);
    if (account) {
        account_alloc();
    }
    dream_ptr ptr = (dream_ptr)(block + (int32_t)HEAP_HEADER_SIZE);
    dream_set_type(ptr, info);
    return ptr;
}

static dream_ptr finish_block_ex(int32_t block, int32_t tag, int32_t account) {
    return activate_block(block, tag, NULL, account);
}

static dream_ptr finish_block(int32_t block, int32_t tag) {
    return finish_block_ex(block, tag, 1);
}

static void free_large_locked(int32_t block, int32_t sz);

/* Adopts a free block of at least half a slab, so the tails that exiting workers hand back
 * become the next instance's slab instead of every instance claiming fresh memory. */
static int32_t take_slab_locked(void) {
    int32_t curr = *large_head();
    while (curr) {
        int32_t bsize = dream_i32(curr)[0];
        if (bsize >= PRIV_SLAB / 2) {
            large_remove(curr);
            if (bsize > PRIV_SLAB) {
                large_split(curr, bsize, PRIV_SLAB);
            }
            return curr;
        }
        curr = blk_next(curr);
    }
    return 0;
}

/* The abandoned slab tail goes back to the large list instead of leaking until exit. */
static void priv_refill(void) {
    int32_t off = dream_priv_off_get();
    int32_t rest = dream_priv_cap_get() - off;
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    if (rest >= MIN_SPLIT) {
        int32_t tail = dream_priv_slab_get() + off;
        dream_i32(tail)[0] = rest;
        free_large_locked(tail, rest);
    }
    int32_t slab = take_slab_locked();
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
    int32_t cap = PRIV_SLAB;
    if (slab) {
        cap = dream_i32(slab)[0];
    } else {
        slab = dream_wasm_heap_claim(PRIV_SLAB);
    }
    dream_priv_slab_set(slab);
    dream_priv_off_set(0);
    dream_priv_cap_set(cap);
}

/* Blocks past the private classes are freed onto the large list, so they must be taken from
 * it too; carving them from the slab instead grew linear memory on every buffer doubling. */
static int32_t priv_large_block(int32_t total) {
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    int32_t block = take_from_large(total);
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
    if (!block) {
        block = dream_wasm_heap_claim(total);
        dream_i32(block)[0] = total;
    }
    return block;
}

static dream_ptr malloc_private_ex(int32_t size, int32_t tag, int32_t account) {
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
        int32_t *table = priv_table();
        block = table[idx];
        if (block) {
            next = blk_next(block);
            table[idx] = next;
            return finish_block_ex(block, tag, account);
        }
    } else {
        return finish_block_ex(priv_large_block(total), tag, account);
    }
    off = dream_priv_off_get();
    cap = dream_priv_cap_get();
    if (cap - off < total) {
        priv_refill();
        off = 0;
    }
    slab = dream_priv_slab_get();
    block = slab + off;
    dream_priv_off_set(off + total);
    dream_i32(block)[0] = total;
    return finish_block_ex(block, tag, account);
}

static dream_ptr malloc_private(int32_t size, int32_t tag) {
    return malloc_private_ex(size, tag, 1);
}

dream_ptr dream_region_backing_malloc(int32_t size) {
    return dream_malloc_shared(size, 0);
}

dream_ptr dream_region_activate(char *block, int32_t total, int32_t tag, const dream_type_info *info) {
    int32_t address = (int32_t)(uintptr_t)block;
    dream_i32(address)[0] = total;
    dream_ptr ptr = finish_block(address, tag);
    dream_set_type(ptr, info);
    return ptr;
}

void dream_region_account_free(uint32_t count) {
    account_free_n((int32_t)count);
}

void dream_region_heap_mode(int active) {
    dream_priv_fast_set(active ? 0 : dream_priv_table_get());
}

static dream_ptr malloc_locked(int32_t size, int32_t tag, int account) {
#ifdef DREAM_COMPONENT
    dream_heap_init();
#endif
    int32_t idx;
    int32_t *head;
    int32_t block = 0;
    int32_t next;

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
        block = dream_wasm_heap_claim(size);
        dream_i32(block)[0] = size;
    } else if (dream_i32(block)[0] >= size + MIN_SPLIT) {
        /* Split a reused block much bigger than this request. */
        large_split(block, dream_i32(block)[0], size);
    }

    return finish_block_ex(block, tag, account);
}

static __attribute__((noinline)) dream_ptr malloc_slow(int32_t size, int32_t tag);

/* Small enough to inline at every allocation site after the runtime is linked with the
 * program; everything else (regions, shared, refills, large blocks) stays out of line. */
DREAM_WASM_EXPORT(DREAM_SYM_MALLOC) __attribute__((always_inline))
dream_ptr dream_malloc(int32_t size, int32_t tag) {
    int32_t *table = (int32_t *)(uintptr_t)(uint32_t)dream_priv_fast_get();
    if (DREAM_LIKELY(table != NULL && (uint32_t)size <= PRIV_SMALL_TOTAL - HEAP_HEADER_SIZE
                     && (tag & TAG_VALUE_MASK) != 0 && (tag & TAG_SHARED) == 0)) {
        int32_t total = (((size + 7) & -8) + (int32_t)HEAP_HEADER_SIZE + 15) & -16;
        int32_t idx = size_class(total);
        int32_t block = table[idx];
        if (DREAM_LIKELY(block != 0)) {
            table[idx] = blk_next(block);
            return activate_block(block, tag, dream_type_info_for_tag(tag & TAG_VALUE_MASK), 1);
        }
    }
    return malloc_slow(size, tag);
}

static __attribute__((noinline)) dream_ptr malloc_slow(int32_t size, int32_t tag) {
    if ((tag & TAG_VALUE_MASK) == 0 || (tag & TAG_SHARED)) {
        return dream_malloc_shared(size, tag);
    }
    /* A region allocation already carries its descriptor. */
    dream_ptr pointer = dream_region_try_malloc(size, tag);
    if (pointer != 0) {
        return pointer;
    }
    pointer = malloc_private(size, tag);
    dream_set_type(pointer, dream_type_info_for_tag(tag & TAG_VALUE_MASK));
    return pointer;
}

dream_ptr dream_malloc_shared(int32_t size, int32_t tag) {
    dream_ptr p;
    if (tag != 0) {
        tag |= TAG_SHARED;
    }
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    p = malloc_locked(size, tag, 1);
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
    dream_set_type(p, dream_type_info_for_tag(tag & TAG_VALUE_MASK));
    return p;
}

/* Runtime allocations never enter guest diagnostics, including the interval between
 * allocating storage and constructing the aligned libc pointer on another worker. */
DREAM_WASM_EXPORT("dream_wasm_raw_malloc")
dream_ptr dream_wasm_raw_malloc(int32_t size) {
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    dream_ptr p = malloc_locked(size, 0, 0);
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
    return p;
}

DREAM_WASM_EXPORT("dream_wasm_raw_free")
void dream_wasm_raw_free(dream_ptr ptr) {
    if (!ptr) { return; }
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    recycle_locked(ptr, 0);
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
}

/* The private slab and freelists are reachable only through this instance's globals, so a
 * worker that discards its instance hands them back first; otherwise every Task leaked a slab. */
DREAM_WASM_EXPORT("dream_heap_thread_exit")
void dream_heap_thread_exit(void) {
    int32_t table = dream_priv_table_get();
    int32_t off = dream_priv_off_get();
    int32_t rest = dream_priv_cap_get() - off;
    int32_t slab = dream_priv_slab_get();
    dream_priv_fast_set(0);
    dream_priv_table_set(0);
    dream_priv_slab_set(0);
    dream_priv_off_set(0);
    dream_priv_cap_set(0);
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    if (rest >= MIN_SPLIT) {
        dream_i32(slab + off)[0] = rest;
        free_large_locked(slab + off, rest);
    }
    if (table) {
        int32_t *heads = (int32_t *)(uintptr_t)(uint32_t)table;
        for (int32_t idx = 0; idx < PRIV_CLASSES; idx++) {
            for (int32_t block = heads[idx]; block;) {
                int32_t next = blk_next(block);
                class_push(idx, block);
                block = next;
            }
        }
        recycle_locked(table, 0);
    }
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
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
        sz += dream_i32(curr)[0];
        curr = blk_next(curr);
    }
    /* Then merge into the preceding neighbor when it ends exactly at us. The forward
     * neighbor (already folded into `sz`) must be unlinked here too — it sits between
     * `prev` and `curr` in the list, and its memory now belongs to the merged block. */
    if (prev && prev + dream_i32(prev)[0] == block) {
        dream_i32(prev)[0] = dream_i32(prev)[0] + sz;
        blk_set_next(prev, curr);
        return;
    }
    if (prev) {
        blk_set_next(prev, block);
    } else {
        *large_head() = block;
    }
    blk_set_next(block, curr);
    dream_i32(block)[0] = sz;
}

static void recycle_locked(dream_ptr ptr, int account) {
    int32_t block_start;
    int32_t idx;
    int32_t sz;
    if (dream_region_owns(ptr)) {
        return;
    }
    block_start = (int32_t)ptr - (int32_t)HEAP_HEADER_SIZE;
    sz = dream_i32(block_start)[0];
    if (sz == 0) {
        return;
    }
    __atomic_store_n(dream_rc_word(ptr), 0, __ATOMIC_RELAXED);
    dream_set_type(ptr, NULL);
    if (account) { account_free_n(1); }
    free_list_head = block_start;
    idx = size_class(sz);
    /* Private large blocks share this address-ordered list with the shared subheap.
     * Isolation is TAG_SHARED / atomic RC, not a partitioned address space. */
    if (idx > 12 || sz != class_bytes(idx)) {
        free_large_locked(block_start, sz);
    } else {
        class_push(idx, block_start);
    }
}

static __attribute__((noinline)) void recycle_slow(dream_ptr ptr);

__attribute__((always_inline))
void dream_recycle(dream_ptr ptr) {
    if (!ptr) {
        return;
    }
    int32_t *table = (int32_t *)(uintptr_t)(uint32_t)dream_priv_fast_get();
    int32_t block = (int32_t)ptr - (int32_t)HEAP_HEADER_SIZE;
    int32_t sz = dream_i32(block)[0];
    int32_t tag = *dream_tag_word(ptr);
    /* Exact private class blocks are the powers of two from 16 bytes to PRIV_SMALL_TOTAL. */
    if (DREAM_LIKELY(table != NULL && (tag & (DREAM_TAG_WEAK_TARGET | TAG_SHARED)) == 0
                     && (uint32_t)(sz - 16) <= (uint32_t)(PRIV_SMALL_TOTAL - 16) && (sz & (sz - 1)) == 0)) {
        __atomic_store_n(dream_rc_word(ptr), 0, __ATOMIC_RELAXED);
        dream_set_type(ptr, NULL);
        account_free_n(1);
        int32_t idx = size_class(sz);
        blk_set_next(block, table[idx]);
        table[idx] = block;
        return;
    }
    recycle_slow(ptr);
}

static __attribute__((noinline)) void recycle_slow(dream_ptr ptr) {
    int32_t block_start;
    int32_t idx;
    int32_t sz;
    if (*dream_tag_word(ptr) & DREAM_TAG_WEAK_TARGET) {
        dream_weak_clear_all(ptr);
    }
    if (dream_region_owns(ptr)) {
        return;
    }
    if (dream_tag_shared(ptr)) {
        dream_platform_current->lock(DREAM_LOCK_HEAP);
        recycle_locked(ptr, 1);
        dream_platform_current->unlock(DREAM_LOCK_HEAP);
        return;
    }
    block_start = (int32_t)ptr - (int32_t)HEAP_HEADER_SIZE;
    sz = dream_i32(block_start)[0];
    if (sz == 0) {
        return;
    }
    __atomic_store_n(dream_rc_word(ptr), 0, __ATOMIC_RELAXED);
    dream_set_type(ptr, NULL);
    account_free_n(1);
    idx = size_class(sz);
    if (idx <= 12 && sz == class_bytes(idx)) {
        priv_class_push(idx, block_start);
        return;
    }
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    free_large_locked(block_start, sz);
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
}

static void retain_copied_edge(dream_ptr child, void *context) {
    (void)context;
    dream_retain(child);
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
    old_total = dream_i32(block_start)[0];
    new_total = round_total(new_size);
    if (new_total <= old_total) {
        return ptr;
    }
    const dream_type_info *info = dream_object_info(ptr);
    np = dream_tag_shared(ptr) ? dream_malloc_shared(new_size, tag) : dream_malloc(new_size, tag);
    dream_set_type(np, info);
    int unique = dream_rc_count(ptr) == 1;
    if (!unique && info && info->visit) {
        dream_visit_owned(ptr, retain_copied_edge, NULL);
    }
    copy = old_total - (int32_t)HEAP_HEADER_SIZE;
    if (copy > new_size) {
        copy = new_size;
    }
    memcpy(dream_p(np), dream_p(ptr), (size_t)copy);
    if (unique && tag == TAG_ARRAY && info) {
        /* The copied payload owns the transferred edges before the old storage is released. */
        memset(dream_p(ptr), 0, (size_t)copy);
    }
    dream_release(ptr);
    return np;
}
