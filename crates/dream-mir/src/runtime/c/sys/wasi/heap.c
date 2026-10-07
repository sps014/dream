#include "dream_heap_internal.h"
#include "dream_platform_internal.h"
#include "dream_region.h"

#include <limits.h>

#ifdef DREAM_WASM32_THREADS
int dream_rt_mt = 1;
#else
int dream_rt_mt;
#endif
int64_t live_objects;
int64_t total_allocations;
int64_t dream_raw_live_objects;
int64_t dream_raw_total_allocations;
int32_t last_freed;
int32_t free_list_head;

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
    return dream_wasm32_meta_i32(META_FL + i * 4);
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
    int64_t v;
    int64_t next;
    __atomic_fetch_add(&last_freed, n, __ATOMIC_RELAXED);
    if (n <= 0) {
        return;
    }
    for (;;) {
        v = __atomic_load_n(&live_objects, __ATOMIC_RELAXED);
        next = v > n ? v - n : 0;
        if (__atomic_compare_exchange_n(
                &live_objects, &v, next, 0, __ATOMIC_RELAXED, __ATOMIC_RELAXED
            )) {
            return;
        }
    }
}

static void priv_class_push(int32_t idx, int32_t block) {
    int32_t head = dream_priv_fl_get(idx);
    blk_set_next(block, head);
    dream_priv_fl_set(idx, block);
}

static dream_ptr finish_block_ex(int32_t block, int32_t tag, int32_t account) {
    i32_put(block + (int32_t)HEADER_TAG_OFFSET, tag);
    i32_put(block + (int32_t)HEADER_REFCOUNT_OFFSET, dream_rc_init(tag));
    if (account) {
        account_alloc();
    }
    dream_ptr ptr = (dream_ptr)(block + (int32_t)HEAP_HEADER_SIZE);
    const dream_type_info *info = NULL;
    memcpy((char *)dream_p(ptr) - DREAM_BLOCK_HEADER + sizeof(dream_size), &info, sizeof(info));
    return ptr;
}

static dream_ptr finish_block(int32_t block, int32_t tag) {
    return finish_block_ex(block, tag, 1);
}

static void priv_refill(int32_t need) {
    int32_t n = need > PRIV_SLAB ? need : PRIV_SLAB;
    n = (n + 15) & -16;
    dream_priv_slab_set(dream_wasm_heap_claim(n));
    dream_priv_off_set(0);
    dream_priv_cap_set(n);
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
        block = dream_priv_fl_get(idx);
        if (block) {
            next = blk_next(block);
            dream_priv_fl_set(idx, next);
            return finish_block_ex(block, tag, account);
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
    i32_put(address, total);
    dream_ptr ptr = finish_block(address, tag);
    dream_set_type(ptr, info);
    return ptr;
}

void dream_region_account_free(uint32_t count) {
    account_free_n((int32_t)count);
}

void dream_region_heap_mode(int active) {
    (void)active;
}

static dream_ptr malloc_locked(int32_t size, int32_t tag) {
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
        i32_put(block, size);
    } else if (i32_at(block) >= size + MIN_SPLIT) {
        /* Split a reused block much bigger than this request. */
        large_split(block, i32_at(block), size);
    }

    i32_put(block + (int32_t)HEADER_TAG_OFFSET, tag);
    i32_put(block + (int32_t)HEADER_REFCOUNT_OFFSET, dream_rc_init(tag));
    account_alloc();
    dream_ptr ptr = (dream_ptr)(block + (int32_t)HEAP_HEADER_SIZE);
    const dream_type_info *info = NULL;
    memcpy((char *)dream_p(ptr) - DREAM_BLOCK_HEADER + sizeof(dream_size), &info, sizeof(info));
    return ptr;
}

int64_t debug_get_live_objects(void) {
    return __atomic_load_n(&live_objects, __ATOMIC_RELAXED) -
        __atomic_load_n(&dream_raw_live_objects, __ATOMIC_RELAXED);
}
int64_t debug_get_total_allocations(void) {
    return __atomic_load_n(&total_allocations, __ATOMIC_RELAXED) -
        __atomic_load_n(&dream_raw_total_allocations, __ATOMIC_RELAXED);
}
int32_t debug_get_ref_count(dream_ptr ptr) {
    return ptr ? dream_rc_count(ptr) : 0;
}

void dream_pin_immortal(dream_ptr s) {
    if (!s) { return; }
    int locked = dream_cycle_store_begin(s, 0, 1);
    if (__atomic_exchange_n(dream_rc_word(s), DREAM_RC_IMMORTAL, __ATOMIC_RELAXED) != DREAM_RC_IMMORTAL) {
        dream_cycle_forget(s);
        int64_t live = __atomic_load_n(&live_objects, __ATOMIC_RELAXED);
        while (live > 0 && !__atomic_compare_exchange_n(
            &live_objects, &live, live - 1, 0, __ATOMIC_RELAXED, __ATOMIC_RELAXED)) {
        }
    }
    dream_cycle_store_end(locked);
}

void dream_retain_slow(int32_t *rc, int32_t v) {
    if (v == DREAM_RC_IMMORTAL) {
        return;
    }
    __atomic_fetch_add(rc, 1, __ATOMIC_RELAXED);
}

int dream_rc_last_slow(int32_t *rc, int32_t v) {
    if (v == 0 || v == DREAM_RC_IMMORTAL) {
        return 0;
    }
    if (__atomic_fetch_sub(rc, 1, __ATOMIC_ACQ_REL) == (DREAM_RC_SHARED_BIT | 1)) {
        __atomic_store_n(rc, 0, __ATOMIC_RELAXED);
        return 1;
    }
    return 0;
}
int32_t debug_get_heap_ptr(void) { return dream_wasm_heap_ptr_get(); }
/* Native parity: the probe exposes "most recent freed block" (a free-happened detector),
 * not this allocator's internal list head, which coalescing keeps stable. */
int32_t debug_get_free_list_head(void) { return last_freed; }

__attribute__((export_name(DREAM_SYM_MALLOC)))
dream_ptr dream_malloc(int32_t size, int32_t tag) {
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

dream_ptr dream_malloc_private(int32_t size, int32_t tag, const dream_type_info *untracked) {
    dream_ptr pointer = dream_region_try_malloc_private(size, tag, untracked);
    return pointer != 0 ? pointer : dream_malloc(size, tag);
}

dream_ptr dream_malloc_shared(int32_t size, int32_t tag) {
    dream_ptr p;
    if (tag != 0) {
        tag |= TAG_SHARED;
    }
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    p = malloc_locked(size, tag);
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
    dream_set_type(p, dream_type_info_for_tag(tag & TAG_VALUE_MASK));
    return p;
}

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
    return rc != 0 && rc != DREAM_RC_IMMORTAL;
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
    if (dream_region_owns(ptr)) {
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
    /* Private large blocks share this address-ordered list with the shared subheap.
     * Isolation is TAG_SHARED / atomic RC, not a partitioned address space. */
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
    dream_cycle_forget(ptr);
    if (*dream_tag_word(ptr) & DREAM_TAG_WEAK_TARGET) {
        dream_weak_clear_all(ptr);
    }
    if (dream_region_owns(ptr)) {
        return;
    }
    if (dream_tag_shared(ptr)) {
        dream_platform_current->lock(DREAM_LOCK_HEAP);
        recycle_locked(ptr);
        dream_platform_current->unlock(DREAM_LOCK_HEAP);
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
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    free_large_locked(block_start, sz);
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
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
    old_total = i32_at(block_start);
    new_total = round_total(new_size);
    if (new_total <= old_total) {
        return ptr;
    }
    const dream_type_info *info = dream_object_info(ptr);
    int locked = info && info->visit;
    if (locked) { dream_cycle_enter(); }
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
    if (locked) { dream_cycle_leave(); dream_cycle_drain(); }
    return np;
}
