/* libc/Dream-heap bridge for wasm32 PCRE2. `malloc`/`free` are 1-arg libc;
 * `malloc_tagged`/`free_tagged` are the Dream `$malloc`/`$free` (size, tag). */
#include "include/dream_abi.h"
#include <stddef.h>
#include <stdint.h>

__attribute__((import_module("env"), import_name("malloc"))) int32_t malloc_tagged(int32_t size,
                                                                                   int32_t tag);
__attribute__((import_module("env"), import_name("free"))) void free_tagged(int32_t ptr);

void *__wrap_malloc(size_t n) {
    int32_t raw;
    size_t *hdr;
    if (n == 0) {
        n = 1;
    }
    raw = malloc_tagged((int32_t)(n + 8), TAG_ARRAY);
    hdr = (size_t *)(uintptr_t)(uint32_t)raw;
    hdr[0] = n;
    return (void *)(hdr + 1);
}

void __wrap_free(void *p) {
    if (p) {
        free_tagged((int32_t)(uintptr_t)((size_t *)p - 1));
    }
}

void *__wrap_memcpy(void *dst, const void *src, size_t n);

void *__wrap_realloc(void *p, size_t n) {
    void *q;
    size_t old = 0;
    size_t c;
    if (n == 0) {
        __wrap_free(p);
        return NULL;
    }
    if (p) {
        old = ((size_t *)p)[-1];
    }
    q = __wrap_malloc(n);
    if (q && p) {
        c = old < n ? old : n;
        __wrap_memcpy(q, p, c);
        __wrap_free(p);
    }
    return q;
}

void *__wrap_memcpy(void *dst, const void *src, size_t n) {
    uint8_t *d = (uint8_t *)dst;
    const uint8_t *s = (const uint8_t *)src;
    size_t i;
    for (i = 0; i < n; i++) {
        d[i] = s[i];
    }
    return dst;
}

void *__wrap_memmove(void *dst, const void *src, size_t n) {
    uint8_t *d = (uint8_t *)dst;
    const uint8_t *s = (const uint8_t *)src;
    size_t i;
    if (d == s || n == 0) {
        return dst;
    }
    if (d < s) {
        for (i = 0; i < n; i++) {
            d[i] = s[i];
        }
    } else {
        i = n;
        while (i) {
            i--;
            d[i] = s[i];
        }
    }
    return dst;
}

void *__wrap_memset(void *dst, int c, size_t n) {
    uint8_t *d = (uint8_t *)dst;
    size_t i;
    for (i = 0; i < n; i++) {
        d[i] = (uint8_t)c;
    }
    return dst;
}

int __wrap_memcmp(const void *a, const void *b, size_t n) {
    const uint8_t *x = (const uint8_t *)a;
    const uint8_t *y = (const uint8_t *)b;
    size_t i;
    for (i = 0; i < n; i++) {
        if (x[i] != y[i]) {
            return (int)x[i] - (int)y[i];
        }
    }
    return 0;
}

size_t __wrap_strlen(const char *s) {
    size_t n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

void __wrap_abort(void) {
    __builtin_trap();
}
