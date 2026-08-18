#include "include/dream_rt_native.h"

#include <pthread.h>
#include <stddef.h>
#include <stdlib.h>

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
static char *arena;
static size_t arena_off;
static size_t arena_len;
static int32_t live_objects;
static int32_t total_allocations;
static int32_t last_freed;
static char *chunks[32];
static int nchunks;
static pthread_mutex_t heap_mu = PTHREAD_MUTEX_INITIALIZER;
int dream_rt_mt;

/* One exclusive block per size class: alloc/free of short-lived strings (arc_locals,
 * substring slices) skip the locked freelist after the first churn. */
static _Thread_local char *tls_free[NCLASS];

static void heap_lock(void) {
    if (dream_rt_mt) {
        pthread_mutex_lock(&heap_mu);
    }
}

static void heap_unlock(void) {
    if (dream_rt_mt) {
        pthread_mutex_unlock(&heap_mu);
    }
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

static char *bump(size_t n) {
    size_t aligned = (n + 15u) & ~15u;
    if (arena == NULL || arena_off + aligned > arena_len) {
        arena = (char *)map_chunk(CHUNK);
        arena_off = 0;
        arena_len = CHUNK;
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

dream_ptr dream_malloc(int32_t size, int32_t tag) {
    int32_t total = ((size + 15) & -16) + 16;
    int idx = size_class(total);
    char *block = NULL;
    int32_t alloc_size = total;
    if (idx >= 0 && idx <= 12) {
        alloc_size = class_bytes(idx);
        block = tls_free[idx];
        if (block != NULL) {
            tls_free[idx] = NULL;
            ((uint32_t *)block)[1] = MAGIC_LIVE;
            ((int32_t *)block)[2] = tag;
            ((int32_t *)block)[3] = 1;
            live_objects += 1;
            total_allocations += 1;
            return (dream_ptr)(block + 16);
        }
    }
    heap_lock();
    if (idx >= 0 && idx <= 12) {
        while (freelist[idx] != 0) {
            block = (char *)dream_p(freelist[idx]);
            if (((uint32_t *)block)[1] != MAGIC_FREE) {
                freelist[idx] = 0;
                block = NULL;
                break;
            }
            {
                void *next = NULL;
                memcpy(&next, block + 8, sizeof(next));
                if (next == (void *)block) {
                    next = NULL;
                }
                freelist[idx] = (dream_ptr)(uintptr_t)next;
            }
            break;
        }
    }
    if (block == NULL) {
        block = bump((size_t)alloc_size);
        ((int32_t *)block)[0] = alloc_size;
    }
    ((uint32_t *)block)[1] = MAGIC_LIVE;
    ((int32_t *)block)[2] = tag;
    ((int32_t *)block)[3] = 1;
    live_objects += 1;
    total_allocations += 1;
    heap_unlock();
    return (dream_ptr)(block + 16);
}

int32_t debug_get_live_objects(void) { return live_objects; }
int32_t debug_get_total_allocations(void) { return total_allocations; }
int32_t debug_get_ref_count(dream_ptr ptr) {
    return ptr ? __atomic_load_n((int32_t *)((char *)dream_p(ptr) - 4), __ATOMIC_RELAXED) : 0;
}
int32_t debug_get_heap_ptr(void) { return (int32_t)arena_off; }
int32_t debug_get_free_list_head(void) { return last_freed; }

void dream_free(dream_ptr ptr) {
    char *block;
    int32_t sz;
    int idx;
    if (ptr == 0) {
        return;
    }
    dream_str_fini(ptr);
    if (dream_weak_any) {
        dream_weak_clear_all(ptr);
    }
    block = (char *)dream_p(ptr) - 16;
    sz = ((int32_t *)block)[0];
    if (sz == 0 || ((uint32_t *)block)[1] != MAGIC_LIVE) {
        return;
    }
    idx = size_class(sz);
    if (idx > 12) {
        idx = 12;
    }
    if (tls_free[idx] == NULL) {
        ((uint32_t *)block)[1] = MAGIC_FREE;
        last_freed += 1;
        if (live_objects > 0) {
            live_objects -= 1;
        }
        tls_free[idx] = block;
        return;
    }
    heap_lock();
    ((uint32_t *)block)[1] = MAGIC_FREE;
    {
        void *next = dream_p(freelist[idx]);
        if (next == (void *)block) {
            next = NULL;
        }
        memcpy(block + 8, &next, sizeof(next));
    }
    freelist[idx] = (dream_ptr)block;
    last_freed += 1;
    if (live_objects > 0) {
        live_objects -= 1;
    }
    heap_unlock();
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
    np = dream_malloc(new_size, tag);
    copy = old_total - 16;
    if (copy > new_size) {
        copy = new_size;
    }
    dream_mem_copy(np, ptr, (size_t)copy);
    dream_free(ptr);
    return np;
}
