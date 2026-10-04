#include <stddef.h>
#include <stdint.h>
#if defined(DREAM_WASM32)
/* `__builtin_memmove`/`__builtin_memset` lower to single `memory.copy` / `memory.fill`
 * instructions (the runtime is always compiled with -mbulk-memory; see src/driver/wasi.rs) — one
 * trap check instead of a byte loop per element. `memory.copy` has memmove semantics. */
void *memcpy(void *dst, const void *src, size_t n) {
    if (n) {
        __builtin_memmove(dst, src, n);
    }
    return dst;
}

void *memset(void *dst, int c, size_t n) {
    if (n) {
        __builtin_memset(dst, c, n);
    }
    return dst;
}

void *memmove(void *dst, const void *src, size_t n) {
    return memcpy(dst, src, n);
}

int memcmp(const void *a, const void *b, size_t n) {
    const uint8_t *x = (const uint8_t *)a;
    const uint8_t *y = (const uint8_t *)b;
    while (n >= 8) {
        uint64_t xa;
        uint64_t ya;
        __builtin_memcpy(&xa, x, 8);
        __builtin_memcpy(&ya, y, 8);
        if (xa != ya) {
            const uint8_t *xd = (const uint8_t *)&xa;
            const uint8_t *yd = (const uint8_t *)&ya;
            int i;
            for (i = 0; i < 8; i++) {
                if (xd[i] != yd[i]) {
                    return (int)xd[i] - (int)yd[i];
                }
            }
        }
        x += 8;
        y += 8;
        n -= 8;
    }
    while (n--) {
        if (*x != *y) {
            return (int)*x - (int)*y;
        }
        x++;
        y++;
    }
    return 0;
}

#elif !__STDC_HOSTED__
/* Volatile byte accesses keep the compiler from lowering these freestanding definitions
 * back into a call to themselves on targets without bulk-memory instructions. */
void *memcpy(void *dst, const void *src, size_t n) {
    volatile unsigned char *d = dst;
    const volatile unsigned char *s = src;
    for (size_t i = 0; i < n; ++i) { d[i] = s[i]; }
    return dst;
}
void *memset(void *dst, int value, size_t n) {
    volatile unsigned char *d = dst;
    for (size_t i = 0; i < n; ++i) { d[i] = (unsigned char)value; }
    return dst;
}
void *memmove(void *dst, const void *src, size_t n) {
    volatile unsigned char *d = dst;
    const volatile unsigned char *s = src;
    if ((uintptr_t)d < (uintptr_t)s) {
        for (size_t i = 0; i < n; ++i) { d[i] = s[i]; }
    } else {
        while (n) { --n; d[n] = s[n]; }
    }
    return dst;
}
int memcmp(const void *a, const void *b, size_t n) {
    const unsigned char *x = a, *y = b;
    for (size_t i = 0; i < n; ++i) { if (x[i] != y[i]) { return x[i] - y[i]; } }
    return 0;
}
#endif
#if defined(DREAM_WASM32) || !__STDC_HOSTED__
size_t strlen(const char *s) {
    size_t n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

#endif
