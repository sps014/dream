#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_heap_maps.h"
#include "dream_region.h"

#include <limits.h>
#include <stddef.h>

#define NCLASS DREAM_NCLASS
#define CHUNK (1u << 22)

static dream_ptr freelist[NCLASS];
static int shared_available[NCLASS];
/* First-fit list for blocks larger than the biggest size class (64KiB), matching
 * wasm `$malloc`'s huge list. Oversized HTTP bodies / `byte[]`s must not be stuffed
 * into the top class or bump-allocated past a 4MiB mmap. */
static dream_ptr large_freelist;
static char *arena;
static size_t arena_off;
static size_t arena_len;
/* Registry of every thread's counters (see `dream_heap_counters`); guarded by the platform heap lock.
 * `pinned` counts immortal singletons that left `Debug.live_objects` without being freed. */
static dream_heap_counters *counters_head;
static uint64_t retired_allocs;
static uint64_t retired_frees;
static uint64_t pinned;
uint64_t dream_weak_allocations;
uint64_t dream_weak_frees;

int dream_rt_mt;

/* Per-thread LIFO of exact size-class blocks (`dream_heap.free`) plus the fast-path gate.
 * Tree/array churn stays off the process-wide list. */
_Thread_local dream_heap_tls dream_heap;

/* Slow-path thread state in one block: region-heavy code runs every alloc/free through here,
 * and each distinct `_Thread_local` is its own TLV lookup on macOS. */
typedef struct {
    dream_heap_counters *counters;
    char *arena;
    size_t arena_off;
    size_t arena_len;
    int region_active;
} heap_thread;
static _Thread_local heap_thread th;

typedef struct HeapRecord {
    dream_heap_counters counters;
    struct HeapRecord *next;
    char *tail;
    size_t tail_size;
} HeapRecord;
static HeapRecord *retired_arenas;

static void heap_lock(void) {
    dream_platform_current->lock(DREAM_LOCK_HEAP);
}

static void heap_unlock(void) {
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
}

/* Class index for a block of `size` total bytes, or NCLASS when it is a large block. */
static int size_class(size_t size) {
    if (size - 1u >= DREAM_MAX_CLASS_BYTES) {
        return NCLASS;
    }
    return dream_size_class((uint32_t)size);
}

static size_t class_bytes(int idx) {
    return dream_class_bytes(idx);
}

static dream_heap_counters *thread_counters(void) {
    dream_heap_counters *c = th.counters;
    if (c != NULL) {
        return c;
    }
    heap_lock();
    HeapRecord *record = retired_arenas;
    if (record) {
        retired_arenas = record->next;
        th.arena = record->tail;
        th.arena_off = 0;
        th.arena_len = record->tail_size;
    }
    heap_unlock();
    if (!record) {
        record = dream_raw_calloc(1, sizeof(*record));
        if (!record) { DREAM_PANIC_LITERAL(u"panic: out of memory registering heap counters"); }
    }
    c = &record->counters;
    memset(c, 0, sizeof(*c));
    heap_lock();
    c->next = counters_head;
    counters_head = c;
    heap_unlock();
    th.counters = c;
    return c;
}

static void heap_sums(uint64_t *allocs, uint64_t *frees) {
    dream_heap_counters *c;
    uint64_t a;
    uint64_t f;
    heap_lock();
    a = retired_allocs;
    f = retired_frees;
    for (c = counters_head; c != NULL; c = c->next) {
        a += __atomic_load_n(&c->allocs, __ATOMIC_RELAXED);
        f += __atomic_load_n(&c->frees, __ATOMIC_RELAXED);
    }
    f += __atomic_load_n(&pinned, __ATOMIC_RELAXED);
    a += __atomic_load_n(&dream_weak_allocations, __ATOMIC_RELAXED);
    f += __atomic_load_n(&dream_weak_frees, __ATOMIC_RELAXED);
    heap_unlock();
    *allocs = a;
    *frees = f;
}

void dream_pin_immortal(dream_ptr s) {
    if (!s) { return; }
    if (__atomic_exchange_n(dream_rc_word(s), DREAM_RC_IMMORTAL, __ATOMIC_RELAXED) != DREAM_RC_IMMORTAL) {
#if DREAM_HEAP_COUNTING
        __atomic_fetch_add(&pinned, 1u, __ATOMIC_RELAXED);
#endif
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

static void note_heap_map(char *p, size_t n) {
    heap_lock();
    dream_heap_map_add_locked(p, n);
    heap_unlock();
}

static int native_block_in_heap(char *block) {
    heap_lock();
    int found = dream_heap_map_contains_locked(block, NATIVE_HEAP_HEADER_SIZE);
    heap_unlock();
    return found;
}

static void *map_chunk(size_t n) { return dream_platform_current->map(n); }

static void *block_next(char *block) {
    void *next = (void *)dream_block_word(block);
    return next == (void *)block ? NULL : next;
}

static void block_set_next(char *block, void *next) {
    *dream_block_tag(block) = DREAM_TAG_FREED;
    dream_block_set_word(block, (uintptr_t)next);
}

void dream_heap_thread_exit(void) {
    HeapRecord *record = (HeapRecord *)th.counters;
    if (!record) { return; }
    if (th.region_active) { DREAM_PANIC_LITERAL(u"panic: thread exit with an active allocation region"); }
    heap_lock();
    for (unsigned cls = 0; cls < NCLASS; cls++) {
        char *block = dream_heap.free[cls];
        while (block) {
            char *next = block_next(block);
            block_set_next(block, dream_p(freelist[cls]));
            freelist[cls] = (dream_ptr)block;
            block = next;
        }
        __atomic_store_n(&shared_available[cls], freelist[cls] != 0, __ATOMIC_RELEASE);
    }
    dream_heap_counters **link = &counters_head;
    while (*link != &record->counters) { link = &(*link)->next; }
    *link = record->counters.next;
    retired_allocs += record->counters.allocs;
    retired_frees += record->counters.frees;
    /* Live published objects belong to their mappings, not to this thread's counters or tail. */
    record->tail_size = th.arena_len - th.arena_off;
    if (record->tail_size >= NATIVE_HEAP_HEADER_SIZE) {
        record->tail = th.arena + th.arena_off;
        record->next = retired_arenas;
        retired_arenas = record;
        record = NULL;
    }
    heap_unlock();
    if (record) { dream_platform_current->deallocate(record); }
    memset(&dream_heap, 0, sizeof(dream_heap));
    memset(&th, 0, sizeof(th));
}

/* Lays out a large block of `bytes`, prefix included, at `p`. */
static char *carve_large(char *p, size_t bytes) {
    dream_block_init(p, DREAM_CLASS_LARGE_PREFIX);
    p += DREAM_LARGE_PREFIX;
    *dream_large_size(p) = bytes - DREAM_LARGE_PREFIX;
    return p;
}

static void activate(char *block, unsigned cls, int32_t tag) {
    dream_block_activate(block, cls, tag);
    dream_heap_count(&thread_counters()->allocs, UINT64_C(1));
}

static void account_frees(uint32_t n) {
    dream_heap_counters *c = thread_counters();
    dream_heap_count(&c->frees, (uint64_t)n);
}

/* Re-arm the inline fast path once the thread is registered and no region is open. */
static void heap_refresh_fast(void);

static char *large_try_take(size_t need) {
    dream_ptr prev = 0;
    dream_ptr curr = large_freelist;
    while (curr != 0) {
        char *block = (char *)dream_p(curr);
        void *next = block_next(block);
        if (*dream_large_size(block) >= need) {
            if (prev == 0) {
                large_freelist = (dream_ptr)next;
            } else {
                block_set_next((char *)dream_p(prev), next);
            }
            return block;
        }
        prev = curr;
        curr = (dream_ptr)next;
    }
    return NULL;
}

static char *tls_bump(size_t n) {
    size_t aligned = (n + 15u) & ~(size_t)15u;
    if (th.arena == NULL || aligned > th.arena_len || th.arena_off > th.arena_len - aligned) {
        size_t map_len = aligned > (size_t)CHUNK ? aligned : (size_t)CHUNK;
        th.arena = (char *)map_chunk(map_len);
        th.arena_off = 0;
        th.arena_len = map_len;
        if (th.arena == NULL) {
            DREAM_PANIC_LITERAL(u"panic: out of memory mapping the private heap");
        }
        note_heap_map(th.arena, map_len);
    }
    {
        char *p = th.arena + th.arena_off;
        th.arena_off += aligned;
        return p;
    }
}

static char *bump(size_t n) {
    size_t aligned = (n + 15u) & ~(size_t)15u;
    if (arena == NULL || aligned > arena_len || arena_off > arena_len - aligned) {
        size_t map_len = aligned > (size_t)CHUNK ? aligned : (size_t)CHUNK;
        arena = (char *)map_chunk(map_len);
        arena_off = 0;
        arena_len = map_len;
        if (arena == NULL) {
            DREAM_PANIC_LITERAL(u"panic: out of memory mapping the shared heap");
        }
        dream_heap_map_add_locked(arena, map_len);
    }
    {
        char *p = arena + arena_off;
        arena_off += aligned;
        return p;
    }
}

dream_ptr dream_region_activate(char *block, dream_size total, int32_t tag, const dream_type_info *info) {
    unsigned cls;
    if (DREAM_LIKELY((size_t)total <= 256u)) {
        cls = (unsigned)(((size_t)total - 1u) >> 4);
    } else if ((size_t)total > DREAM_MAX_CLASS_BYTES) {
        cls = DREAM_CLASS_LARGE;
        block = carve_large(block, (size_t)total);
    } else {
        cls = (unsigned)dream_size_class((uint32_t)total);
    }
    dream_block_activate_info(block, cls, tag, info);
    dream_heap_count(&thread_counters()->allocs, UINT64_C(1));
    return (dream_ptr)(block + NATIVE_HEAP_HEADER_SIZE);
}

void dream_region_account_free(uint32_t count) {
    account_frees(count);
}

void dream_region_heap_mode(int active) {
    th.region_active = active;
    dream_heap.fast = NULL;
    heap_refresh_fast();
}

static void heap_refresh_fast(void) {
    if (dream_heap.fast == NULL && !th.region_active) {
        dream_heap.fast = thread_counters();
    }
}

static size_t allocation_total(size_t size) {
    if (size > PTRDIFF_MAX - NATIVE_HEAP_HEADER_SIZE - 15) {
        DREAM_PANIC_LITERAL(u"panic: allocation size exceeds the supported limit");
    }
    return ((size + 15u) & ~(size_t)15u) + NATIVE_HEAP_HEADER_SIZE;
}

static dream_ptr malloc_general(dream_size size, int32_t tag) {
    size_t total;
    int idx;
    unsigned cls;
    char *block = NULL;
    size_t alloc_size;
    size_t footprint;
    total = allocation_total(size);
    heap_refresh_fast();
    idx = size_class(total);
    cls = idx < NCLASS ? (unsigned)idx : DREAM_CLASS_LARGE;
    alloc_size = idx < NCLASS ? class_bytes(idx) : total;
    if (idx < NCLASS) {
        block = dream_heap.free[idx];
        if (block != NULL) {
            dream_heap.free[idx] = block_next(block);
            activate(block, cls, tag);
            return (dream_ptr)(block + NATIVE_HEAP_HEADER_SIZE);
        }
        if (__atomic_load_n(&shared_available[idx], __ATOMIC_ACQUIRE)) {
            heap_lock();
            for (unsigned count = 0; count < 32 && freelist[idx]; count++) {
                char *cached = dream_p(freelist[idx]);
                freelist[idx] = (dream_ptr)block_next(cached);
                block_set_next(cached, dream_heap.free[idx]);
                dream_heap.free[idx] = cached;
            }
            __atomic_store_n(&shared_available[idx], freelist[idx] != 0, __ATOMIC_RELEASE);
            heap_unlock();
        }
        block = dream_heap.free[idx];
        if (block) {
            dream_heap.free[idx] = block_next(block);
            activate(block, cls, tag);
            return (dream_ptr)(block + NATIVE_HEAP_HEADER_SIZE);
        }
    } else {
        /* `dream_recycle_slow` returns every large block, private or shared, to the
         * process-wide list; bump-allocating past it would never reuse freed buffers. */
        heap_lock();
        block = large_try_take(alloc_size);
        heap_unlock();
        if (block != NULL) {
            activate(block, cls, tag);
            return (dream_ptr)(block + NATIVE_HEAP_HEADER_SIZE);
        }
    }
    if (cls == DREAM_CLASS_LARGE) {
        footprint = alloc_size + DREAM_LARGE_PREFIX;
        block = carve_large(tls_bump(footprint), footprint);
    } else {
        block = tls_bump(alloc_size);
    }
    activate(block, cls, tag);
    return (dream_ptr)(block + NATIVE_HEAP_HEADER_SIZE);
}

dream_ptr dream_malloc_slow(dream_size size, int32_t tag) {
    dream_ptr pointer = dream_region_try_malloc(size, tag);
    return pointer != 0 ? pointer : malloc_general(size, tag);
}

dream_ptr dream_region_backing_malloc(dream_size size) {
    return dream_malloc_shared(size, 0);
}

dream_ptr dream_malloc_shared(dream_size size, int32_t tag) {
    size_t total;
    int idx;
    unsigned cls;
    char *block = NULL;
    size_t alloc_size;
    size_t footprint;
    total = allocation_total(size);
    idx = size_class(total);
    cls = idx < NCLASS ? (unsigned)idx : DREAM_CLASS_LARGE;
    alloc_size = idx < NCLASS ? class_bytes(idx) : total;
    if (tag != 0) {
        tag |= TAG_SHARED;
    }
    heap_lock();
    if (idx >= NCLASS) {
        block = large_try_take(alloc_size);
    } else {
        while (freelist[idx] != 0) {
            block = (char *)dream_p(freelist[idx]);
            if (*dream_block_tag(block) != DREAM_TAG_FREED) {
                freelist[idx] = 0;
                block = NULL;
                break;
            }
            freelist[idx] = (dream_ptr)block_next(block);
            break;
        }
        __atomic_store_n(&shared_available[idx], freelist[idx] != 0, __ATOMIC_RELEASE);
    }
    if (block == NULL) {
        if (cls == DREAM_CLASS_LARGE) {
            footprint = alloc_size + DREAM_LARGE_PREFIX;
            block = carve_large(bump(footprint), footprint);
        } else {
            block = bump(alloc_size);
        }
    }
    heap_unlock();
    activate(block, cls, tag);
    return (dream_ptr)(block + NATIVE_HEAP_HEADER_SIZE);
}

int dream_heap_is_live(dream_ptr ptr) {
    char *block;
    if (ptr == 0 || ((uintptr_t)ptr & (sizeof(dream_ptr) - 1)) != 0) {
        return 0;
    }
    block = (char *)dream_p(ptr) - (int)NATIVE_HEAP_HEADER_SIZE;
    if (!native_block_in_heap(block)) {
        return 0;
    }
    return *dream_block_tag(block) != DREAM_TAG_FREED;
}

int64_t debug_get_live_objects(void) {
    uint64_t a;
    uint64_t f;
    heap_sums(&a, &f);
    return a > f ? (int64_t)(a - f) : 0;
}
int64_t debug_get_total_allocations(void) {
    uint64_t a;
    uint64_t f;
    heap_sums(&a, &f);
    return (int64_t)a;
}

int32_t debug_get_ref_count(dream_ptr ptr) {
    return ptr ? dream_rc_count(ptr) : 0;
}
/* Both probes return `int`; a value past INT32_MAX saturates rather than wrapping negative. */
static int32_t saturate_i32(uint64_t value) {
    return value > (uint64_t)INT32_MAX ? INT32_MAX : (int32_t)value;
}
int32_t debug_get_heap_ptr(void) { return saturate_i32((uint64_t)arena_off); }
int32_t debug_get_free_list_head(void) {
    uint64_t a;
    uint64_t f;
    heap_sums(&a, &f);
    return saturate_i32(f - __atomic_load_n(&pinned, __ATOMIC_RELAXED));
}

void dream_recycle_slow(dream_ptr ptr) {
    char *block;
    size_t sz;
    unsigned idx;
    if (ptr == 0) {
        return;
    }
    heap_refresh_fast();
    if (*dream_tag_word(ptr) == DREAM_TAG_FREED) {
        return;
    }
    if (*dream_tag_word(ptr) & DREAM_TAG_WEAK_TARGET) {
        dream_weak_clear_all(ptr);
    }
    block = (char *)dream_p(ptr) - NATIVE_HEAP_HEADER_SIZE;
    if (dream_region_owns(ptr)) {
        return;
    }
    idx = dream_block_class(block);
    sz = dream_block_total(block);
    int shared = dream_tag_shared(ptr);
    if (shared) {
        dream_platform_current->object_drop(dream_p(ptr));
    }
    DREAM_POISON(block + NATIVE_HEAP_HEADER_SIZE, sz - NATIVE_HEAP_HEADER_SIZE);
    account_frees(1);
    if (shared || idx >= NCLASS) {
        heap_lock();
        if (idx >= NCLASS) {
            block_set_next(block, dream_p(large_freelist));
            large_freelist = (dream_ptr)block;
        } else {
            block_set_next(block, dream_p(freelist[idx]));
            freelist[idx] = (dream_ptr)block;
            __atomic_store_n(&shared_available[idx], 1, __ATOMIC_RELEASE);
        }
        heap_unlock();
        return;
    }
    block_set_next(block, dream_heap.free[idx]);
    dream_heap.free[idx] = block;
}

void dream_free(dream_ptr ptr) {
    if (ptr == 0) {
        return;
    }
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

dream_ptr dream_realloc(dream_ptr ptr, dream_size new_size, int32_t tag) {
    char *block;
    size_t old_total;
    size_t new_total;
    dream_ptr np;
    size_t copy;
    new_total = allocation_total(new_size);
    if (ptr == 0) {
        return dream_malloc(new_size, tag);
    }
    block = (char *)dream_p(ptr) - NATIVE_HEAP_HEADER_SIZE;
    old_total = dream_block_total(block);
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
    copy = old_total - NATIVE_HEAP_HEADER_SIZE;
    if (copy > new_size) {
        copy = new_size;
    }
    dream_mem_copy(np, ptr, (size_t)copy);
    /* Share-aware move: the slot's +1 transfers to the new block, but read-derived
     * aliases (retained field/index snapshots) may still hold their own +1 on the
     * old block. Release instead of freeing outright so those aliases stay valid;
     * with a single holder this is exactly dream_free. */
    if (unique && tag == TAG_ARRAY && info) {
        /* The copied payload owns the transferred edges before the old storage is released. */
        memset(dream_p(ptr), 0, (size_t)copy);
    }
    dream_release(ptr);
    return np;
}
