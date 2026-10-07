#include "dream_rt_wasm32.h"
#include <stddef.h>
#include <errno.h>

/* Native libc allocations do not contribute to Dream object diagnostics. */
extern int64_t dream_raw_live_objects;
extern int64_t dream_raw_total_allocations;

static void *allocate_aligned(size_t n, size_t alignment) {
    if (alignment > (size_t)INT32_MAX - 8 || n > (size_t)INT32_MAX - alignment - 8) {
        return NULL;
    }
    /* C and C++ allocations require max_align_t alignment, unlike Dream's compact payloads. */
    dream_ptr raw = dream_malloc((int32_t)(n + alignment + 8), 0);
    uintptr_t aligned = ((uintptr_t)(uint32_t)raw + 8 + alignment - 1) & ~(uintptr_t)(alignment - 1);
    ((uint32_t *)aligned)[-2] = (uint32_t)raw;
    ((uint32_t *)aligned)[-1] = (uint32_t)n;
    __atomic_fetch_add(&dream_raw_live_objects, 1, __ATOMIC_RELAXED);
    __atomic_fetch_add(&dream_raw_total_allocations, 1, __ATOMIC_RELAXED);
    return (void *)aligned;
}

void *malloc(size_t n) { return allocate_aligned(n, 16); }

int posix_memalign(void **out, size_t alignment, size_t n) {
    if (alignment < sizeof(void *) || (alignment & (alignment - 1))) { return EINVAL; }
    void *p = allocate_aligned(n, alignment);
    if (!p) { return ENOMEM; }
    *out = p;
    return 0;
}

void *aligned_alloc(size_t alignment, size_t n) {
    if (!alignment || (alignment & (alignment - 1)) || n % alignment) { return NULL; }
    return allocate_aligned(n, alignment < 16 ? 16 : alignment);
}

void *calloc(size_t n, size_t sz) {
    size_t bytes;
    void *p;
    if (sz != 0 && n > (size_t)INT32_MAX / sz) {
        return NULL;
    }
    bytes = n * sz;
    p = malloc(bytes);
    if (p) {
        memset(p, 0, bytes);
    }
    return p;
}

void free(void *p) {
    if (p) {
        __atomic_fetch_sub(&dream_raw_live_objects, 1, __ATOMIC_RELAXED);
        dream_free((dream_ptr)((uint32_t *)p)[-2]);
    }
}

void *realloc(void *p, size_t n) {
    if (!n) { free(p); return NULL; }
    void *next = malloc(n);
    if (next && p) {
        size_t old = ((uint32_t *)p)[-1];
        memcpy(next, p, old < n ? old : n);
        free(p);
    }
    return next;
}

/* WASI libc calls these internal entry points from strdup and other archive members. */
void *__libc_malloc(size_t n) { return malloc(n); }
void *__libc_calloc(size_t n, size_t size) { return calloc(n, size); }
void __libc_free(void *p) { free(p); }
size_t malloc_usable_size(void *p) { return p ? ((uint32_t *)p)[-1] : 0; }
