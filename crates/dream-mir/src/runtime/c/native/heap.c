#include "include/dream_rt_native.h"

#include <limits.h>
#include <pthread.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#include <windows.h>
#else
#include <sys/mman.h>
#include <unistd.h>
#endif

#define NCLASS 13
#define CHUNK (1u << 22)
#define MAGIC_LIVE 0x4c495645u
#define MAGIC_FREE 0x46524545u

static dream_ptr freelist[NCLASS];
/* First-fit list for blocks larger than the biggest size class (64KiB), matching
 * wasm `$malloc`'s huge list. Oversized HTTP bodies / `byte[]`s must not be stuffed
 * into class 12 or bump-allocated past a 4MiB mmap. */
static dream_ptr large_freelist;
static char *arena;
static size_t arena_off;
static size_t arena_len;
/* Global (mirrors wasm32): immortal singletons in strings.c/format.c adjust it when pinning. */
int32_t live_objects;
static int32_t total_allocations;
static int32_t last_freed;
static char *chunks[32];
static int nchunks;
static pthread_mutex_t heap_mu = PTHREAD_MUTEX_INITIALIZER;
int dream_rt_mt;

/* Per-thread LIFO of exact size-class blocks. Tree/array churn stays off the process-wide
 * list (the old one-slot TLS overflowed after the first free of each class). */
static _Thread_local char *tls_free[NCLASS];
static _Thread_local char *tls_arena;
static _Thread_local size_t tls_arena_off;
static _Thread_local size_t tls_arena_len;

/* Unique-graph bump region: mallocs while depth > 0 come from a rewindable TLS slab so
 * `dream_region_leave` reclaims the whole graph in O(1). Independent of the process arena so
 * workers cannot rewind each other's bump pointer. */
#define REGION_MAX_DEPTH 8
#define REGION_CHUNK (1u << 23)
static _Thread_local int region_depth;
static _Thread_local char *region_base;
static _Thread_local size_t region_len;
static _Thread_local size_t region_off;
static _Thread_local int32_t region_nalloc;
static _Thread_local size_t region_off_mark[REGION_MAX_DEPTH];
static _Thread_local int32_t region_nalloc_mark[REGION_MAX_DEPTH];

static int region_owns_block(char *block) {
    return region_depth > 0 && region_base != NULL && block >= region_base
        && (size_t)(block - region_base) < region_len;
}

static void heap_lock(void) {
    pthread_mutex_lock(&heap_mu);
}

static void heap_unlock(void) {
    pthread_mutex_unlock(&heap_mu);
}

static int size_class(int32_t size) {
    int32_t s = size;
    if (s < 16) {
        s = 16;
    }
    return 28 - __builtin_clz((unsigned)(s - 1));
}

static int32_t class_bytes(int idx) {
    if (idx > 12) {
        return 0;
    }
    return 1 << (idx + 4);
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

static void account_alloc(void) {
    __atomic_fetch_add(&live_objects, 1, __ATOMIC_RELAXED);
    __atomic_fetch_add(&total_allocations, 1, __ATOMIC_RELAXED);
}

static void live_objects_sub(int32_t n) {
    int32_t v;
    int32_t next;
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

static void account_free(void) {
    __atomic_fetch_add(&last_freed, 1, __ATOMIC_RELAXED);
    live_objects_sub(1);
}

static void activate(char *block, int32_t tag) {
    ((uint32_t *)block)[1] = MAGIC_LIVE;
    ((int32_t *)block)[2] = tag;
    ((int32_t *)block)[3] = 1;
    account_alloc();
}

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
    if (tls_arena == NULL || aligned > tls_arena_len || tls_arena_off > tls_arena_len - aligned) {
        size_t map_len = aligned > (size_t)CHUNK ? aligned : (size_t)CHUNK;
        tls_arena = (char *)map_chunk(map_len);
        tls_arena_off = 0;
        tls_arena_len = map_len;
        if (tls_arena == NULL) {
            abort();
        }
    }
    {
        char *p = tls_arena + tls_arena_off;
        tls_arena_off += aligned;
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
    if (region_base == NULL) {
        region_base = (char *)map_chunk(REGION_CHUNK);
        if (region_base == NULL) {
            abort();
        }
        region_len = REGION_CHUNK;
        region_off = 0;
    }
    if (region_off > region_len || n > region_len - region_off) {
        abort();
    }
    block = region_base + region_off;
    region_off += n;
    ((int32_t *)block)[0] = alloc_size;
    activate(block, tag);
    region_nalloc += 1;
    return (dream_ptr)(block + 16);
}

void dream_region_enter(void) {
    if (region_depth >= REGION_MAX_DEPTH) {
        abort();
    }
    region_off_mark[region_depth] = region_off;
    region_nalloc_mark[region_depth] = region_nalloc;
    region_depth += 1;
}

void dream_region_leave(void) {
    int32_t n;
    if (region_depth <= 0) {
        return;
    }
    region_depth -= 1;
    n = region_nalloc - region_nalloc_mark[region_depth];
    if (n < 0) {
        n = 0;
    }
    __atomic_fetch_add(&last_freed, n, __ATOMIC_RELAXED);
    live_objects_sub(n);
    region_nalloc = region_nalloc_mark[region_depth];
    region_off = region_off_mark[region_depth];
}

dream_ptr dream_malloc(int32_t size, int32_t tag) {
    int32_t total;
    int idx;
    char *block = NULL;
    int32_t alloc_size;
    if (size < 0 || size > (INT32_MAX - 31)) {
        abort();
    }
    total = ((size + 15) & -16) + 16;
    idx = size_class(total);
    alloc_size = total;
    if (idx >= 0 && idx <= 12) {
        alloc_size = class_bytes(idx);
    }
    if (region_depth > 0) {
        return region_malloc(alloc_size, tag);
    }
    if (idx >= 0 && idx <= 12) {
        block = tls_free[idx];
        if (block != NULL) {
            tls_free[idx] = class_next(block);
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
    alloc_size = total;
    if (idx >= 0 && idx <= 12) {
        alloc_size = class_bytes(idx);
    }
    if (tag != 0) {
        tag |= TAG_SHARED;
    }
    heap_lock();
    if (idx > 12) {
        block = large_try_take(alloc_size);
    } else if (idx >= 0 && idx <= 12) {
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
    activate(block, tag);
    heap_unlock();
    return (dream_ptr)(block + 16);
}

#define PUBLISH_SEEN_MAX 256

static int native_is_live_ptr(dream_ptr ptr) {
    char *block;
    if (ptr == 0 || (ptr & (sizeof(dream_ptr) - 1)) != 0) {
        return 0;
    }
    block = (char *)dream_p(ptr) - (int)NATIVE_HEAP_HEADER_SIZE;
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
        if (native_is_live_ptr(child)) {
            publish_rec(child, seen, nseen);
        }
    }
}

static void publish_rec(dream_ptr ptr, dream_ptr *seen, int *nseen) {
    int32_t *tag;
    int32_t kind;
    int i;
    if (!native_is_live_ptr(ptr)) {
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
    return __atomic_load_n(&live_objects, __ATOMIC_RELAXED);
}
int32_t debug_get_total_allocations(void) {
    return __atomic_load_n(&total_allocations, __ATOMIC_RELAXED);
}
int32_t debug_get_ref_count(dream_ptr ptr) {
    return ptr ? __atomic_load_n((int32_t *)((char *)dream_p(ptr) - 4), __ATOMIC_RELAXED) : 0;
}
int32_t debug_get_heap_ptr(void) { return (int32_t)arena_off; }
int32_t debug_get_free_list_head(void) { return last_freed; }

void dream_recycle(dream_ptr ptr) {
    char *block;
    int32_t sz;
    int idx;
    if (ptr == 0) {
        return;
    }
    if (dream_weak_any) {
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
    account_free();
    if (dream_tag_shared(ptr) || idx > 12) {
        heap_lock();
        if (idx > 12) {
            large_set_next(block, dream_p(large_freelist));
            large_freelist = (dream_ptr)block;
        } else {
            class_set_next(block, dream_p(freelist[idx]));
            freelist[idx] = (dream_ptr)block;
        }
        heap_unlock();
        return;
    }
    class_set_next(block, tls_free[idx]);
    tls_free[idx] = block;
}

void dream_free(dream_ptr ptr) {
    if (ptr == 0) {
        return;
    }
    dream_str_fini(ptr);
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
