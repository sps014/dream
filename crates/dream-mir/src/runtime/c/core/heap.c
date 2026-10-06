#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_heap_maps.h"
#include "dream_region.h"

#include <limits.h>
#include <stddef.h>

#define NCLASS DREAM_NCLASS
#define CHUNK (1u << 22)
#define MAGIC_LIVE DREAM_MAGIC_LIVE
#define MAGIC_FREE DREAM_MAGIC_FREE

static dream_ptr freelist[NCLASS];
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
static uint64_t pinned;

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
    c = (dream_heap_counters *)dream_raw_calloc(1, sizeof(*c));
    if (c == NULL) {
        DREAM_PANIC_LITERAL(u"panic: out of memory registering heap counters");
    }
    heap_lock();
    c->next = counters_head;
    counters_head = c;
    heap_unlock();
    th.counters = c;
    return c;
}

static void heap_sums(uint64_t *allocs, uint64_t *frees) {
    dream_heap_counters *c;
    uint64_t a = 0;
    uint64_t f = 0;
    heap_lock();
    for (c = counters_head; c != NULL; c = c->next) {
        a += __atomic_load_n(&c->allocs, __ATOMIC_RELAXED);
        f += __atomic_load_n(&c->frees, __ATOMIC_RELAXED);
    }
    f += __atomic_load_n(&pinned, __ATOMIC_RELAXED);
    heap_unlock();
    *allocs = a;
    *frees = f;
}

void dream_pin_immortal(dream_ptr s) {
    if (s) {
        *dream_rc_word(s) = DREAM_RC_IMMORTAL;
        __atomic_fetch_add(&pinned, 1u, __ATOMIC_RELAXED);
    }
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

static void *large_next(char *block) {
    void *next = NULL;
    memcpy(&next, block + 8, sizeof(next));
    return next;
}

static void large_set_next(char *block, void *next) {
    memcpy(block + 8, &next, sizeof(next));
}

static void *class_next(char *block) {
    void *next = NULL;
    memcpy(&next, block + 8, sizeof(next));
    if (next == (void *)block) {
        return NULL;
    }
    return next;
}

static void class_set_next(char *block, void *next) {
    memcpy(block + 8, &next, sizeof(next));
}

static void activate(char *block, int32_t tag) {
    dream_block_activate(block, tag);
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
        void *next = large_next(block);
        if (*dream_block_magic(block) == MAGIC_FREE && *dream_block_size(block) >= need) {
            if (prev == 0) {
                large_freelist = (dream_ptr)next;
            } else {
                large_set_next((char *)dream_p(prev), next);
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

dream_ptr dream_region_activate(char *block, dream_size total, int32_t tag) {
    *dream_block_size(block) = total;
    activate(block, tag);
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
    char *block = NULL;
    size_t alloc_size;
    total = allocation_total(size);
    heap_refresh_fast();
    idx = size_class(total);
    alloc_size = idx < NCLASS ? class_bytes(idx) : total;
    if (idx < NCLASS) {
        block = dream_heap.free[idx];
        if (block != NULL) {
            dream_heap.free[idx] = class_next(block);
            activate(block, tag);
            return (dream_ptr)(block + NATIVE_HEAP_HEADER_SIZE);
        }
    }
    block = tls_bump((size_t)alloc_size);
    *dream_block_size(block) = alloc_size;
    activate(block, tag);
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
    char *block = NULL;
    size_t alloc_size;
    total = allocation_total(size);
    idx = size_class(total);
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
            if (*dream_block_magic(block) != MAGIC_FREE) {
                freelist[idx] = 0;
                block = NULL;
                break;
            }
            {
                void *next = class_next(block);
                freelist[idx] = (dream_ptr)next;
            }
            break;
        }
    }
    if (block == NULL) {
        block = bump((size_t)alloc_size);
        *dream_block_size(block) = alloc_size;
    }
    heap_unlock();
    activate(block, tag);
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
    return *dream_block_magic(block) == MAGIC_LIVE;
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
int32_t debug_get_heap_ptr(void) { return (int32_t)arena_off; }
int32_t debug_get_free_list_head(void) {
    uint64_t a;
    uint64_t f;
    heap_sums(&a, &f);
    return (int32_t)(f - __atomic_load_n(&pinned, __ATOMIC_RELAXED));
}

void dream_recycle_slow(dream_ptr ptr) {
    char *block;
    size_t sz;
    int idx;
    if (ptr == 0) {
        return;
    }
    heap_refresh_fast();
    if (*dream_tag_word(ptr) & DREAM_TAG_WEAK_TARGET) {
        dream_weak_clear_all(ptr);
    }
    block = (char *)dream_p(ptr) - NATIVE_HEAP_HEADER_SIZE;
    if (dream_region_owns(ptr)) {
        return;
    }
    sz = *dream_block_size(block);
    if (sz == 0 || *dream_block_magic(block) != MAGIC_LIVE) {
        return;
    }
    if (dream_tag_shared(ptr)) {
        dream_platform_current->object_drop(dream_p(ptr));
    }
    idx = size_class(sz);
    *dream_block_magic(block) = MAGIC_FREE;
    account_frees(1);
    if (dream_tag_shared(ptr) || idx >= NCLASS) {
        heap_lock();
        if (idx >= NCLASS) {
            large_set_next(block, dream_p(large_freelist));
            large_freelist = (dream_ptr)block;
        } else {
            class_set_next(block, dream_p(freelist[idx]));
            freelist[idx] = (dream_ptr)block;
        }
        heap_unlock();
        return;
    }
    class_set_next(block, dream_heap.free[idx]);
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
    old_total = *dream_block_size(block);
    if (new_total <= old_total) {
        return ptr;
    }
    np = dream_tag_shared(ptr) ? dream_malloc_shared(new_size, tag) : dream_malloc(new_size, tag);
    copy = old_total - NATIVE_HEAP_HEADER_SIZE;
    if (copy > new_size) {
        copy = new_size;
    }
    dream_mem_copy(np, ptr, (size_t)copy);
    /* Share-aware move: the slot's +1 transfers to the new block, but read-derived
     * aliases (retained field/index snapshots) may still hold their own +1 on the
     * old block. Release instead of freeing outright so those aliases stay valid;
     * with a single holder this is exactly dream_free. */
    dream_release(ptr);
    return np;
}
