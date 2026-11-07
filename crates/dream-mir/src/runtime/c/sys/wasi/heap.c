#include "dream_heap_internal.h"
#include "dream_platform_internal.h"
#include "dream_region.h"

#include <limits.h>

#ifdef DREAM_WASM32_THREADS
int dream_rt_mt = 1;
#else
int dream_rt_mt;
#endif

/* Class blocks come from homogeneous spans whose class the page map records, so the guest header
 * carries no size. Blocks past DREAM_MAX_CLASS_BYTES live on one address-ordered free list so
 * physical neighbors merge on free. */
enum { MIN_LARGE = 32 };

static void lock_heap(void) { dream_platform_current->lock(DREAM_LOCK_HEAP); }
static void unlock_heap(void) { dream_platform_current->unlock(DREAM_LOCK_HEAP); }

static dream_class_lists *shared_lists(void) {
    return (dream_class_lists *)dream_wasm32_meta_i32(META_CLASS_LISTS);
}

static int32_t *large_head(void) {
    return dream_wasm32_meta_i32(META_LARGE_HEAD);
}

static void mark_pages(int32_t start, int32_t bytes, unsigned kind) {
    uint8_t *map = dream_wasm_page_map();
    for (uint32_t page = (uint32_t)start >> 16; page <= (uint32_t)(start + bytes - 1) >> 16; page++) {
        map[page] = (uint8_t)kind;
    }
}

/* Free class blocks link through their first payload word. */
static int32_t blk_next(int32_t block) {
    return dream_i32(block + (int32_t)HEAP_HEADER_SIZE)[0];
}

static void blk_set_next(int32_t block, int32_t next) {
    dream_i32(block + (int32_t)HEAP_HEADER_SIZE)[0] = next;
}

static int32_t large_size(int32_t prefix) { return dream_i32(prefix)[0]; }
static void large_set_size(int32_t prefix, int32_t size) { dream_i32(prefix)[0] = size; }
static int32_t large_next(int32_t prefix) { return dream_i32(prefix)[1]; }
static void large_set_next(int32_t prefix, int32_t next) { dream_i32(prefix)[1] = next; }

static void large_remove(int32_t prefix) {
    int32_t *head = large_head();
    int32_t curr = *head;
    int32_t prev = 0;
    while (curr && curr != prefix) {
        prev = curr;
        curr = large_next(curr);
    }
    if (!curr) {
        return;
    }
    if (prev) {
        large_set_next(prev, large_next(prefix));
    } else {
        *head = large_next(prefix);
    }
}

/* Free a large block, merging with physically adjacent free neighbors (the large list is
 * address-ordered, so both directions are one walk). */
static void free_large_locked(int32_t prefix, int32_t sz) {
    int32_t curr = *large_head();
    int32_t prev = 0;
    int32_t next_phys = prefix + sz;
    while (curr && curr < prefix) {
        prev = curr;
        curr = large_next(curr);
    }
    if (curr == next_phys) {
        sz += large_size(curr);
        curr = large_next(curr);
    }
    /* The forward neighbor (already folded into `sz`) sits between `prev` and `curr` in the
     * list, so merging into `prev` unlinks it too. */
    if (prev && prev + large_size(prev) == prefix) {
        large_set_size(prev, large_size(prev) + sz);
        large_set_next(prev, curr);
        return;
    }
    if (prev) {
        large_set_next(prev, prefix);
    } else {
        *large_head() = prefix;
    }
    large_set_next(prefix, curr);
    large_set_size(prefix, sz);
}

/* Bytes skipped to align a span become a free large block. */
static void reuse_gap_locked(int32_t gap, int32_t end) {
    if (end - gap < MIN_LARGE) {
        return;
    }
    mark_pages(gap, end - gap, DREAM_PAGE_LARGE);
    free_large_locked(gap, end - gap);
}

/* First fit over the address-ordered large list; the remainder stays free. */
static int32_t take_large_locked(int32_t need) {
    int32_t curr = *large_head();
    while (curr) {
        int32_t bsize = large_size(curr);
        if (bsize >= need) {
            large_remove(curr);
            if (bsize - need >= MIN_LARGE) {
                large_set_size(curr, need);
                free_large_locked(curr + need, bsize - need);
            }
            return curr;
        }
        curr = large_next(curr);
    }
    int32_t prefix = dream_wasm_heap_claim(need);
    large_set_size(prefix, need);
    mark_pages(prefix, need, DREAM_PAGE_LARGE);
    return prefix;
}

static int32_t large_block_locked(int32_t need) {
    return take_large_locked(need + DREAM_LARGE_PREFIX) + DREAM_LARGE_PREFIX;
}

static int32_t bump(dream_class_lists *lists, int idx, int locked) {
    int32_t bytes = dream_class_bytes(idx);
    int32_t block = lists->cursor[idx];
    if (block == 0 || lists->end[idx] - block < bytes) {
        int32_t size = dream_wasm_span_bytes(idx);
        int32_t gap;
        block = dream_wasm_heap_claim_aligned(size, size, &gap);
        mark_pages(block, size, (unsigned)idx + 1u);
        lists->end[idx] = block + size;
        if (block - gap >= MIN_LARGE) {
            if (!locked) { lock_heap(); }
            reuse_gap_locked(gap, block);
            if (!locked) { unlock_heap(); }
        }
    }
    lists->cursor[idx] = block + bytes;
    return block;
}

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

/* Per-instance class lists live in linear memory. `fast` aliases them except while a region is
 * open (and before first use), so the inline paths below fall through with one test. */
static dream_class_lists *priv_table(void) {
    int32_t table = dream_priv_table_get();
    if (table == 0) {
        table = dream_wasm_raw_malloc((int32_t)sizeof(dream_class_lists));
        memset((void *)(uintptr_t)(uint32_t)table, 0, sizeof(dream_class_lists));
        dream_priv_table_set(table);
        if (!dream_region_active()) {
            dream_priv_fast_set(table);
        }
    }
    return (dream_class_lists *)(uintptr_t)(uint32_t)table;
}

DREAM_ALWAYS_INLINE dream_ptr activate_block(int32_t block, int32_t tag, int32_t account) {
    dream_i32(block + (int32_t)HEADER_TAG_OFFSET)[0] = tag;
    dream_i32(block + (int32_t)HEADER_REFCOUNT_OFFSET)[0] = dream_rc_init(tag);
    if (account) {
        account_alloc();
    }
    return (dream_ptr)(block + (int32_t)HEAP_HEADER_SIZE);
}

static int32_t block_need(int32_t size) {
    if (size < 0 || size > INT32_MAX - (int32_t)HEAP_HEADER_SIZE - DREAM_LARGE_PREFIX - 15) {
        DREAM_PANIC_LITERAL(u"panic: allocation size exceeds the WASI heap limit");
    }
    return (int32_t)dream_block_need((size_t)size);
}

/* An instance that finds its list empty first adopts storage other instances handed back. Adoption
 * takes a bounded batch so concurrent workers share the pool and thread exit stays O(batch). */
enum { ADOPT_BATCH = 32 };

static int32_t private_class_block(int idx) {
    dream_class_lists *table = priv_table();
    int32_t block = table->free[idx];
    if (block) {
        table->free[idx] = blk_next(block);
        return block;
    }
    dream_class_lists *shared = shared_lists();
    if (__atomic_load_n(&shared->free[idx], __ATOMIC_RELAXED) != 0
        || __atomic_load_n(&shared->cursor[idx], __ATOMIC_RELAXED) != 0) {
        lock_heap();
        for (int count = 0; count < ADOPT_BATCH && shared->free[idx]; count++) {
            int32_t cached = shared->free[idx];
            shared->free[idx] = blk_next(cached);
            blk_set_next(cached, table->free[idx]);
            table->free[idx] = cached;
        }
        if (table->free[idx] == 0 && shared->cursor[idx] != 0
            && shared->end[idx] - shared->cursor[idx] >= dream_class_bytes(idx)
            && (table->cursor[idx] == 0 || table->end[idx] - table->cursor[idx] < dream_class_bytes(idx))) {
            table->cursor[idx] = shared->cursor[idx];
            table->end[idx] = shared->end[idx];
            shared->cursor[idx] = 0;
            shared->end[idx] = 0;
        }
        unlock_heap();
        block = table->free[idx];
        if (block) {
            table->free[idx] = blk_next(block);
            return block;
        }
    }
    return bump(table, idx, 0);
}

static dream_ptr malloc_private(int32_t size, int32_t tag) {
    int32_t need = block_need(size);
    int32_t block;
    if (need <= DREAM_MAX_CLASS_BYTES) {
        block = private_class_block(dream_size_class((uint32_t)need));
    } else {
        lock_heap();
        block = large_block_locked(need);
        unlock_heap();
    }
    return activate_block(block, tag, 1);
}

dream_ptr dream_region_backing_malloc(int32_t size) {
    return dream_malloc_shared(size, 0);
}

dream_ptr dream_region_activate(char *prefix, int32_t total, int32_t tag) {
    *(size_t *)prefix = (size_t)total - DREAM_REGION_PREFIX;
    int32_t block = (int32_t)(uintptr_t)prefix + (int32_t)DREAM_REGION_PREFIX;
    dream_i32(block + (int32_t)HEADER_TAG_OFFSET)[0] = tag | TAG_REGION;
    dream_i32(block + (int32_t)HEADER_REFCOUNT_OFFSET)[0] = dream_rc_init(tag);
    account_alloc();
    return (dream_ptr)(block + (int32_t)HEAP_HEADER_SIZE);
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
    int32_t need = block_need(size);
    int32_t block;
    if (need <= DREAM_MAX_CLASS_BYTES) {
        int idx = dream_size_class((uint32_t)need);
        dream_class_lists *shared = shared_lists();
        block = shared->free[idx];
        if (block) {
            shared->free[idx] = blk_next(block);
        } else {
            block = bump(shared, idx, 1);
        }
    } else {
        block = large_block_locked(need);
    }
    return activate_block(block, tag, account);
}

static __attribute__((noinline)) dream_ptr malloc_slow(int32_t size, int32_t tag);

/* Small enough to inline at every allocation site after the runtime is linked with the
 * program; everything else (regions, shared, refills, large blocks) stays out of line. */
DREAM_WASM_EXPORT(DREAM_SYM_MALLOC) __attribute__((always_inline))
dream_ptr dream_malloc(int32_t size, int32_t tag) {
    dream_class_lists *table = (dream_class_lists *)(uintptr_t)(uint32_t)dream_priv_fast_get();
    if (DREAM_LIKELY(table != NULL && (uint32_t)size <= DREAM_MAX_CLASS_BYTES - HEAP_HEADER_SIZE
                     && (tag & TAG_VALUE_MASK) != 0 && (tag & TAG_SHARED) == 0)) {
        int idx = dream_size_class((uint32_t)dream_block_need((size_t)size));
        int32_t block = table->free[idx];
        if (DREAM_LIKELY(block != 0)) {
            table->free[idx] = blk_next(block);
            return activate_block(block, tag, 1);
        }
    }
    return malloc_slow(size, tag);
}

static __attribute__((noinline)) dream_ptr malloc_slow(int32_t size, int32_t tag) {
    if ((tag & TAG_VALUE_MASK) == 0 || (tag & TAG_SHARED)) {
        return dream_malloc_shared(size, tag);
    }
    dream_ptr pointer = dream_region_try_malloc(size, tag);
    if (pointer != 0) {
        return pointer;
    }
    return malloc_private(size, tag);
}

dream_ptr dream_malloc_shared(int32_t size, int32_t tag) {
    dream_ptr p;
    if (tag != 0) {
        tag |= TAG_SHARED;
    }
    lock_heap();
    p = malloc_locked(size, tag, 1);
    unlock_heap();
    return p;
}

/* Runtime allocations never enter guest diagnostics, including the interval between
 * allocating storage and constructing the aligned libc pointer on another worker. */
DREAM_WASM_EXPORT("dream_wasm_raw_malloc")
dream_ptr dream_wasm_raw_malloc(int32_t size) {
    lock_heap();
    dream_ptr p = malloc_locked(size, 0, 0);
    unlock_heap();
    return p;
}

static void recycle_locked(dream_ptr ptr, int account);

DREAM_WASM_EXPORT("dream_wasm_raw_free")
void dream_wasm_raw_free(dream_ptr ptr) {
    if (!ptr) { return; }
    lock_heap();
    recycle_locked(ptr, 0);
    unlock_heap();
}

/* An instance's class lists are reachable only through its globals, so a worker that discards
 * its instance hands its free blocks and unused span tails to the shared lists first. */
DREAM_WASM_EXPORT("dream_heap_thread_exit")
void dream_heap_thread_exit(void) {
    int32_t table = dream_priv_table_get();
    dream_priv_fast_set(0);
    dream_priv_table_set(0);
    if (!table) {
        return;
    }
    dream_class_lists *lists = (dream_class_lists *)(uintptr_t)(uint32_t)table;
    dream_class_lists *shared = shared_lists();
    lock_heap();
    for (int idx = 0; idx < DREAM_NCLASS; idx++) {
        for (int32_t block = lists->free[idx]; block;) {
            int32_t next = blk_next(block);
            blk_set_next(block, shared->free[idx]);
            shared->free[idx] = block;
            block = next;
        }
        int32_t cursor = lists->cursor[idx];
        int32_t end = lists->end[idx];
        int32_t bytes = dream_class_bytes(idx);
        if (cursor == 0) {
            continue;
        }
        if (shared->cursor[idx] == 0 || shared->end[idx] - shared->cursor[idx] < end - cursor) {
            int32_t kept = shared->cursor[idx];
            int32_t kept_end = shared->end[idx];
            shared->cursor[idx] = cursor;
            shared->end[idx] = end;
            if (kept == 0) {
                continue;
            }
            cursor = kept;
            end = kept_end;
        }
        for (; end - cursor >= bytes; cursor += bytes) {
            blk_set_next(cursor, shared->free[idx]);
            shared->free[idx] = cursor;
        }
    }
    recycle_locked(table, 0);
    unlock_heap();
}

size_t dream_block_total(char *block) {
    int32_t address = (int32_t)(uintptr_t)block;
    if (*dream_block_tag(block) & TAG_REGION) {
        return *(size_t *)(block - DREAM_REGION_PREFIX);
    }
    unsigned kind = dream_wasm_page_kind(address);
    if (kind == DREAM_PAGE_LARGE) {
        return (size_t)(large_size(address - DREAM_LARGE_PREFIX) - DREAM_LARGE_PREFIX);
    }
    if (kind - 1u < (unsigned)DREAM_NCLASS) {
        return (size_t)dream_class_bytes((int)kind - 1);
    }
    return 0;
}

static void clear_header(dream_ptr ptr) {
    __atomic_store_n(dream_rc_word(ptr), 0, __ATOMIC_RELAXED);
    *dream_tag_word(ptr) = 0;
}

static void recycle_locked(dream_ptr ptr, int account) {
    if (dream_region_owns(ptr)) {
        return;
    }
    int32_t block = (int32_t)ptr - (int32_t)HEAP_HEADER_SIZE;
    unsigned kind = dream_wasm_page_kind(block);
    if (kind == DREAM_PAGE_NONE) {
        return;
    }
    clear_header(ptr);
    if (account) { account_free_n(1); }
    free_list_head = block;
    if (kind == DREAM_PAGE_LARGE) {
        int32_t prefix = block - DREAM_LARGE_PREFIX;
        free_large_locked(prefix, large_size(prefix));
        return;
    }
    dream_class_lists *shared = shared_lists();
    blk_set_next(block, shared->free[kind - 1u]);
    shared->free[kind - 1u] = block;
}

static __attribute__((noinline)) void recycle_slow(dream_ptr ptr);

__attribute__((always_inline))
void dream_recycle(dream_ptr ptr) {
    if (!ptr) {
        return;
    }
    dream_class_lists *table = (dream_class_lists *)(uintptr_t)(uint32_t)dream_priv_fast_get();
    int32_t block = (int32_t)ptr - (int32_t)HEAP_HEADER_SIZE;
    int32_t tag = *dream_tag_word(ptr);
    unsigned idx = dream_wasm_page_kind(block) - 1u;
    if (DREAM_LIKELY(table != NULL && (tag & (DREAM_TAG_WEAK_TARGET | TAG_SHARED | TAG_REGION)) == 0
                     && idx < (unsigned)DREAM_NCLASS
                     && __atomic_load_n(dream_rc_word(ptr), __ATOMIC_RELAXED) != DREAM_RC_IMMORTAL)) {
        clear_header(ptr);
        account_free_n(1);
        blk_set_next(block, table->free[idx]);
        table->free[idx] = block;
        return;
    }
    recycle_slow(ptr);
}

static __attribute__((noinline)) void recycle_slow(dream_ptr ptr) {
    if (__atomic_load_n(dream_rc_word(ptr), __ATOMIC_RELAXED) == DREAM_RC_IMMORTAL) {
        return;
    }
    if (*dream_tag_word(ptr) & DREAM_TAG_WEAK_TARGET) {
        dream_weak_clear_all(ptr);
    }
    if (dream_region_owns(ptr)) {
        return;
    }
    if (dream_tag_shared(ptr)) {
        lock_heap();
        recycle_locked(ptr, 1);
        unlock_heap();
        return;
    }
    int32_t block = (int32_t)ptr - (int32_t)HEAP_HEADER_SIZE;
    unsigned kind = dream_wasm_page_kind(block);
    if (kind == DREAM_PAGE_NONE) {
        return;
    }
    clear_header(ptr);
    account_free_n(1);
    if (kind == DREAM_PAGE_LARGE) {
        int32_t prefix = block - DREAM_LARGE_PREFIX;
        lock_heap();
        free_large_locked(prefix, large_size(prefix));
        unlock_heap();
        return;
    }
    dream_class_lists *table = priv_table();
    blk_set_next(block, table->free[kind - 1u]);
    table->free[kind - 1u] = block;
}

static void retain_copied_edge(dream_ptr child, void *context) {
    (void)context;
    dream_retain(child);
}

dream_ptr dream_realloc(dream_ptr ptr, int32_t new_size, int32_t tag) {
    if (!ptr) {
        return dream_malloc(new_size, tag);
    }
    int32_t old_total = (int32_t)dream_block_total((char *)dream_p(ptr) - HEAP_HEADER_SIZE);
    if (block_need(new_size) <= old_total) {
        return ptr;
    }
    const dream_type_info *info = dream_object_info(ptr);
    dream_ptr np = dream_tag_shared(ptr) ? dream_malloc_shared(new_size, tag) : dream_malloc(new_size, tag);
    dream_set_type(np, info);
    int unique = dream_rc_count(ptr) == 1;
    if (!unique && info && info->visit) {
        dream_visit_owned(ptr, retain_copied_edge, NULL);
    }
    int32_t copy = old_total > (int32_t)HEAP_HEADER_SIZE ? old_total - (int32_t)HEAP_HEADER_SIZE : 0;
    if (copy > new_size) {
        copy = new_size;
    }
    memcpy(dream_p(np), dream_p(ptr), (size_t)copy);
    if (unique && dream_tag_kind(tag) == TAG_ARRAY && info) {
        /* The copied payload owns the transferred edges before the old storage is released. */
        memset(dream_p(ptr), 0, (size_t)copy);
    }
    dream_release(ptr);
    return np;
}
