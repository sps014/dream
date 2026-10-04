#include "dream_rt_wasm32.h"
#include <stddef.h>

void *malloc(size_t n) {
    if (n > (size_t)INT32_MAX) {
        return NULL;
    }
    return dream_p(dream_malloc((int32_t)n, 0));
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
        dream_free((dream_ptr)(uintptr_t)p);
    }
}
