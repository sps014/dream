#include "include/dream_rt_native.h"

#include <limits.h>
#include <pthread.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#include <windows.h>
#else
#include <sys/mman.h>
#include <unistd.h>
#endif

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
/* Registry of every thread's counters (see `dream_heap_counters`); guarded by `heap_mu`.
 * `pinned` counts immortal singletons that left `Debug.live_objects` without being freed. */
static dream_heap_counters *counters_head;
static uint32_t pinned;
static char *chunks[32];
static int nchunks;
/* Every mmap used as a heap (process bump, TLS bump, unique-region). `dream_publish`
 * must not deref `char[]` payload words that happen to look like pointers. */
static char *heap_maps[64];
static size_t heap_map_lens[64];
static int nheap_maps;
static pthread_mutex_t heap_mu = PTHREAD_MUTEX_INITIALIZER;
int dream_rt_mt;

/* Per-thread LIFO of exact size-class blocks (`dream_heap.free`) plus the fast-path gate.
 * Tree/array churn stays off the process-wide list. */
_Thread_local dream_heap_tls dream_heap;

/* Unique-graph bump region: mallocs while depth > 0 come from a rewindable TLS slab so
 * `dream_region_leave` reclaims the whole graph in O(1). Independent of the process arena so
 * workers cannot rewind each other's bump pointer. */
#define REGION_MAX_DEPTH 8
#define REGION_CHUNK (1u << 23)

/* Slow-path thread state in one block: region-heavy code runs every alloc/free through here,
 * and each distinct `_Thread_local` is its own TLV lookup on macOS. */
typedef struct {
    dream_heap_counters *counters;
    char *arena;
    size_t arena_off;
    size_t arena_len;
    int region_depth;
    char *region_base;
    size_t region_len;
    size_t region_off;
    int32_t region_nalloc;
    size_t region_off_mark[REGION_MAX_DEPTH];
    int32_t region_nalloc_mark[REGION_MAX_DEPTH];
} heap_thread;
static _Thread_local heap_thread th;

static int region_owns_block(char *block) {
    return th.region_depth > 0 && th.region_base != NULL && block >= th.region_base
        && (size_t)(block - th.region_base) < th.region_len;
}

static void heap_lock(void) {
    pthread_mutex_lock(&heap_mu);
}

static void heap_unlock(void) {
    pthread_mutex_unlock(&heap_mu);
}

/* Class index for a block of `size` total bytes, or NCLASS when it is a large block. */
static int size_class(int32_t size) {
    if ((uint32_t)(size - 1) >= (uint32_t)DREAM_MAX_CLASS_BYTES) {
        return NCLASS;
    }
    return dream_size_class((uint32_t)size);
}

static int32_t class_bytes(int idx) {
    return dream_class_bytes(idx);
}

static dream_heap_counters *thread_counters(void) {
    dream_heap_counters *c = th.counters;
    if (c != NULL) {
        return c;
    }
    c = (dream_heap_counters *)calloc(1, sizeof(*c));
    if (c == NULL) {
        abort();
    }
    heap_lock();
    c->next = counters_head;
    counters_head = c;
    heap_unlock();
    th.counters = c;
    return c;
}

static void heap_sums(uint32_t *allocs, uint32_t *frees) {
    dream_heap_counters *c;
    uint32_t a = 0;
    uint32_t f = 0;
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

void dream_retain_slow(int32_t *rc) {
    if (*rc == DREAM_RC_IMMORTAL) {
        return;
    }
    __atomic_fetch_add(rc, 1, __ATOMIC_RELAXED);
}

int dream_rc_last_slow(int32_t *rc) {
    int32_t v = *rc;
    if (v == 0 || v == DREAM_RC_IMMORTAL) {
        return 0;
    }
    return __atomic_fetch_sub(rc, 1, __ATOMIC_ACQ_REL) == (DREAM_RC_SHARED_BIT | 1);
}

static void note_heap_map_locked(char *p, size_t n) {
    if (p == NULL || n == 0) {
        return;
    }
    if (nheap_maps < 64) {
        heap_maps[nheap_maps] = p;
        heap_map_lens[nheap_maps] = n;
        nheap_maps += 1;
    }
}

static void note_heap_map(char *p, size_t n) {
    heap_lock();
    note_heap_map_locked(p, n);
    heap_unlock();
}

static int native_block_in_heap(char *block) {
    int i;
    int n;
    heap_lock();
    n = nheap_maps;
    for (i = 0; i < n; i++) {
        char *base = heap_maps[i];
        size_t len = heap_map_lens[i];
        if (block >= base && (size_t)(block - base) < len) {
            heap_unlock();
            return 1;
        }
    }
    heap_unlock();
    return 0;
}

static void *map_chunk(size_t n) {
#if defined(_WIN32)
    void *p = VirtualAlloc(NULL, n, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    return p;
#else
    void *p = mmap(NULL, n, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED) {
        return NULL;
    }
    return p;
#endif
}

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
    dream_heap_count(&thread_counters()->allocs);
}

static void account_frees(uint32_t n) {
    dream_heap_counters *c = thread_counters();
    __atomic_store_n(&c->frees, c->frees + n, __ATOMIC_RELAXED);
}

/* Re-arm the inline fast path once the thread is registered and no region is open. */
static void heap_refresh_fast(void);

static char *large_try_take(int32_t need) {
    dream_ptr prev = 0;
    dream_ptr curr = large_freelist;
    while (curr != 0) {
        char *block = (char *)dream_p(curr);
        void *next = large_next(block);
        if (((uint32_t *)block)[1] == MAGIC_FREE && ((int32_t *)block)[0] >= need) {
            if (prev == 0) {
                large_freelist = (dream_ptr)(uintptr_t)next;
            } else {
                large_set_next((char *)dream_p(prev), next);
            }
            return block;
        }
        prev = curr;
        curr = (dream_ptr)(uintptr_t)next;
    }
    return NULL;
}

static char *tls_bump(size_t n) {
    size_t aligned = (n + 15u) & ~15u;
    if (th.arena == NULL || aligned > th.arena_len || th.arena_off > th.arena_len - aligned) {
        size_t map_len = aligned > (size_t)CHUNK ? aligned : (size_t)CHUNK;
        th.arena = (char *)map_chunk(map_len);
        th.arena_off = 0;
        th.arena_len = map_len;
        if (th.arena == NULL) {
            abort();
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
    size_t aligned = (n + 15u) & ~15u;
    if (arena == NULL || aligned > arena_len || arena_off > arena_len - aligned) {
        size_t map_len = aligned > (size_t)CHUNK ? aligned : (size_t)CHUNK;
        arena = (char *)map_chunk(map_len);
        arena_off = 0;
        arena_len = map_len;
        if (arena == NULL) {
            abort();
        }
        if (nchunks < 32) {
            chunks[nchunks++] = arena;
        }
        note_heap_map_locked(arena, map_len);
    }
    {
        char *p = arena + arena_off;
        arena_off += aligned;
        return p;
    }
}

static dream_ptr region_malloc(int32_t alloc_size, int32_t tag) {
    char *block;
    size_t n = (size_t)alloc_size;
    if (th.region_base == NULL) {
        th.region_base = (char *)map_chunk(REGION_CHUNK);
        if (th.region_base == NULL) {
            abort();
        }
        note_heap_map(th.region_base, REGION_CHUNK);
        th.region_len = REGION_CHUNK;
        th.region_off = 0;
    }
    if (th.region_off > th.region_len || n > th.region_len - th.region_off) {
        abort();
    }
    block = th.region_base + th.region_off;
    th.region_off += n;
    ((int32_t *)block)[0] = alloc_size;
    activate(block, tag);
    th.region_nalloc += 1;
    return (dream_ptr)(block + 16);
}

void dream_region_enter(void) {
    if (th.region_depth >= REGION_MAX_DEPTH) {
        abort();
    }
    th.region_off_mark[th.region_depth] = th.region_off;
    th.region_nalloc_mark[th.region_depth] = th.region_nalloc;
    th.region_depth += 1;
    dream_heap.fast = NULL;
}

void dream_region_leave(void) {
    int32_t n;
    if (th.region_depth <= 0) {
        return;
    }
    th.region_depth -= 1;
    n = th.region_nalloc - th.region_nalloc_mark[th.region_depth];
    if (n < 0) {
        n = 0;
    }
    account_frees((uint32_t)n);
    th.region_nalloc = th.region_nalloc_mark[th.region_depth];
    th.region_off = th.region_off_mark[th.region_depth];
    heap_refresh_fast();
}

static void heap_refresh_fast(void) {
    if (dream_heap.fast == NULL && th.region_depth == 0) {
        dream_heap.fast = thread_counters();
    }
}

dream_ptr dream_malloc_slow(int32_t size, int32_t tag) {
    int32_t total;
    int idx;
    char *block = NULL;
    int32_t alloc_size;
    if (size < 0 || size > (INT32_MAX - 31)) {
        abort();
    }
    heap_refresh_fast();
    total = ((size + 15) & -16) + 16;
    idx = size_class(total);
    alloc_size = idx < NCLASS ? class_bytes(idx) : total;
    if (th.region_depth > 0) {
        return region_malloc(alloc_size, tag);
    }
    if (idx < NCLASS) {
        block = dream_heap.free[idx];
        if (block != NULL) {
            dream_heap.free[idx] = class_next(block);
            activate(block, tag);
            return (dream_ptr)(block + 16);
        }
    }
    block = tls_bump((size_t)alloc_size);
    ((int32_t *)block)[0] = alloc_size;
    activate(block, tag);
    return (dream_ptr)(block + 16);
}

dream_ptr dream_malloc_shared(int32_t size, int32_t tag) {
    int32_t total;
    int idx;
    char *block = NULL;
    int32_t alloc_size;
    if (size < 0 || size > (INT32_MAX - 31)) {
        abort();
    }
    total = ((size + 15) & -16) + 16;
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
            if (((uint32_t *)block)[1] != MAGIC_FREE) {
                freelist[idx] = 0;
                block = NULL;
                break;
            }
            {
                void *next = class_next(block);
                freelist[idx] = (dream_ptr)(uintptr_t)next;
            }
            break;
        }
    }
    if (block == NULL) {
        block = bump((size_t)alloc_size);
        ((int32_t *)block)[0] = alloc_size;
    }
    heap_unlock();
    activate(block, tag);
    return (dream_ptr)(block + 16);
}

#define PUBLISH_SEEN_MAX 256

int dream_heap_is_live(dream_ptr ptr) {
    char *block;
    if (ptr == 0 || (ptr & (sizeof(dream_ptr) - 1)) != 0) {
        return 0;
    }
    block = (char *)dream_p(ptr) - (int)NATIVE_HEAP_HEADER_SIZE;
    if (!native_block_in_heap(block)) {
        return 0;
    }
    return ((uint32_t *)block)[1] == MAGIC_LIVE;
}

static void publish_rec(dream_ptr ptr, dream_ptr *seen, int *nseen);

static void publish_walk_payload(dream_ptr ptr, dream_ptr *seen, int *nseen) {
    char *block;
    char *data;
    int32_t sz;
    int32_t payload;
    int32_t off;
    block = (char *)dream_p(ptr) - (int)NATIVE_HEAP_HEADER_SIZE;
    sz = ((int32_t *)block)[0];
    payload = sz - (int32_t)NATIVE_HEAP_HEADER_SIZE;
    data = (char *)dream_p(ptr);
    for (off = 0; off + (int32_t)sizeof(dream_ptr) <= payload; off += (int32_t)sizeof(dream_ptr)) {
        dream_ptr child = 0;
        memcpy(&child, data + off, sizeof(child));
        if (dream_heap_is_live(child)) {
            publish_rec(child, seen, nseen);
        }
    }
}

static void publish_rec(dream_ptr ptr, dream_ptr *seen, int *nseen) {
    int32_t *tag;
    int32_t kind;
    int i;
    if (!dream_heap_is_live(ptr)) {
        return;
    }
    for (i = 0; i < *nseen; i++) {
        if (seen[i] == ptr) {
            return;
        }
    }
    if (*nseen < PUBLISH_SEEN_MAX) {
        seen[(*nseen)++] = ptr;
    }
    tag = (int32_t *)((char *)dream_p(ptr) - TAG_FROM_DATA);
    kind = *tag & TAG_VALUE_MASK;
    if (kind == 0) {
        return;
    }
    *tag |= TAG_SHARED;
    if (*dream_rc_word(ptr) > 0) {
        *dream_rc_word(ptr) |= DREAM_RC_SHARED_BIT;
    }
    if (kind == TAG_STRING) {
        if (dream_i32(ptr)[1] == DREAM_STR_SLICE) {
            dream_ptr parent = 0;
            memcpy(&parent, (char *)dream_p(ptr) + 8, sizeof(parent));
            publish_rec(parent, seen, nseen);
        }
        return;
    }
    if (kind == TAG_ARRAY || kind >= TAG_STRUCT_BASE) {
        publish_walk_payload(ptr, seen, nseen);
    }
}

void dream_publish(dream_ptr ptr) {
    dream_ptr seen[PUBLISH_SEEN_MAX];
    int nseen = 0;
    publish_rec(ptr, seen, &nseen);
}

int32_t debug_get_live_objects(void) {
    uint32_t a;
    uint32_t f;
    int32_t live;
    heap_sums(&a, &f);
    live = (int32_t)(a - f);
    return live > 0 ? live : 0;
}
int32_t debug_get_total_allocations(void) {
    uint32_t a;
    uint32_t f;
    heap_sums(&a, &f);
    return (int32_t)a;
}

__attribute__((weak)) const char *dream_tag_name(int32_t tag) {
    switch (tag & TAG_VALUE_MASK) {
    case 0:
        return "future";
    case TAG_STRING:
        return "string";
    case TAG_ARRAY:
        return "array";
    case TAG_CLOSURE_ENV:
        return "closure_env";
    case TAG_FUNCBOX:
        return "funcbox";
    case TAG_FUTURE:
        return "future";
    default:
        return "object";
    }
}

#define DUMP_HIST 256
#define DUMP_STR_SAMPLES 20
#define DUMP_STR_UNITS 40

typedef struct {
    int32_t tag;
    int32_t n;
} DumpHist;

typedef struct {
    int32_t n;
    uint16_t u[DUMP_STR_UNITS];
} DumpStr;

static void dump_hist_add(DumpHist *h, int *nh, int32_t tag) {
    int i;
    for (i = 0; i < *nh; i++) {
        if (h[i].tag == tag) {
            h[i].n += 1;
            return;
        }
    }
    if (*nh < DUMP_HIST) {
        h[*nh].tag = tag;
        h[*nh].n = 1;
        *nh += 1;
    }
}

static void dump_string_save(char *data, DumpStr *ss, int32_t *printed) {
    int32_t n;
    int32_t i;
    if (*printed >= DUMP_STR_SAMPLES) {
        return;
    }
    n = ((int32_t *)data)[0];
    if (n < 0) {
        n = 0;
    }
    if (n > DUMP_STR_UNITS) {
        n = DUMP_STR_UNITS;
    }
    ss[*printed].n = n;
    for (i = 0; i < n; i++) {
        ss[*printed].u[i] = ((uint16_t *)(data + 8))[i];
    }
    *printed += 1;
}

static void dump_scan_map(char *base, size_t len, DumpHist *h, int *nh, DumpStr *ss,
                          int32_t *str_n) {
    char *p = base;
    char *end = base + len;
    while (p + 16 <= end) {
        int32_t sz = ((int32_t *)p)[0];
        uint32_t mag = ((uint32_t *)p)[1];
        if (sz < 16 || (sz & 15) != 0 || (size_t)sz > (size_t)(end - p)) {
            p += 16;
            continue;
        }
        // Pinned singletons (`dream_pin_immortal`) are never freed by design and already left
        // `live_objects`, so counting them here would report a leak the accounting denies.
        if (mag == MAGIC_LIVE && ((int32_t *)p)[3] != DREAM_RC_IMMORTAL) {
            int32_t tag = ((int32_t *)p)[2] & TAG_VALUE_MASK;
            dump_hist_add(h, nh, tag);
            if (tag == TAG_STRING) {
                dump_string_save(p + 16, ss, str_n);
            }
        }
        p += sz;
    }
}

void debug_dump_live(void) {
    DumpHist hist[DUMP_HIST];
    DumpStr samples[DUMP_STR_SAMPLES];
    int nh = 0;
    int32_t str_n = 0;
    int i;
    int j;
    heap_lock();
    for (i = 0; i < nheap_maps; i++) {
        dump_scan_map(heap_maps[i], heap_map_lens[i], hist, &nh, samples, &str_n);
    }
    heap_unlock();
    for (i = 0; i < nh; i++) {
        int best = i;
        for (j = i + 1; j < nh; j++) {
            if (hist[j].n > hist[best].n) {
                best = j;
            }
        }
        if (best != i) {
            DumpHist tmp = hist[i];
            hist[i] = hist[best];
            hist[best] = tmp;
        }
    }
    fputs("[dream] leak by type:", stderr);
    if (nh == 0) {
        fputs(" (none)\n", stderr);
    } else {
        for (i = 0; i < nh && i < 16; i++) {
            fprintf(stderr, " %s=%d", dream_tag_name(hist[i].tag), hist[i].n);
        }
        fputc('\n', stderr);
    }
    if (str_n > 0) {
        fputs("[dream] leak strings:\n", stderr);
        for (i = 0; i < str_n; i++) {
            fputs("  \"", stderr);
            for (j = 0; j < samples[i].n; j++) {
                unsigned u = samples[i].u[j];
                if (u >= 32 && u < 127 && u != '"' && u != '\\') {
                    fputc((int)u, stderr);
                } else {
                    fputc('?', stderr);
                }
            }
            fputs("\"\n", stderr);
        }
    }
}
int32_t debug_get_ref_count(dream_ptr ptr) {
    return ptr ? dream_rc_count(ptr) : 0;
}
int32_t debug_get_heap_ptr(void) { return (int32_t)arena_off; }
int32_t debug_get_free_list_head(void) {
    uint32_t a;
    uint32_t f;
    heap_sums(&a, &f);
    return (int32_t)(f - __atomic_load_n(&pinned, __ATOMIC_RELAXED));
}

void dream_recycle_slow(dream_ptr ptr) {
    char *block;
    int32_t sz;
    int idx;
    if (ptr == 0) {
        return;
    }
    heap_refresh_fast();
    if (*dream_tag_word(ptr) & DREAM_TAG_WEAK_TARGET) {
        dream_weak_clear_all(ptr);
    }
    block = (char *)dream_p(ptr) - 16;
    if (region_owns_block(block)) {
        return;
    }
    sz = ((int32_t *)block)[0];
    if (sz == 0 || ((uint32_t *)block)[1] != MAGIC_LIVE) {
        return;
    }
    idx = size_class(sz);
    ((uint32_t *)block)[1] = MAGIC_FREE;
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
    dream_str_fini(ptr);
    if (dream_object_tag(ptr) == TAG_FUTURE) {
        dream_future_fini(ptr);
    }
    dream_recycle(ptr);
}

dream_ptr dream_realloc(dream_ptr ptr, int32_t new_size, int32_t tag) {
    char *block;
    int32_t old_total;
    int32_t new_total;
    dream_ptr np;
    int32_t copy;
    if (ptr == 0) {
        return dream_malloc(new_size, tag);
    }
    block = (char *)dream_p(ptr) - 16;
    old_total = ((int32_t *)block)[0];
    new_total = ((new_size + 15) & -16) + 16;
    if ((uint32_t)new_total <= (uint32_t)old_total) {
        return ptr;
    }
    np = dream_tag_shared(ptr) ? dream_malloc_shared(new_size, tag) : dream_malloc(new_size, tag);
    copy = old_total - 16;
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
