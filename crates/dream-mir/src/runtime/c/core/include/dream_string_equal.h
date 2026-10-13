#ifndef DREAM_STRING_EQUAL_H
#define DREAM_STRING_EQUAL_H

DREAM_ALWAYS_INLINE int dream_units_equal(const void *a, const void *b, size_t bytes) {
#if defined(__wasm__)
    /* LLVM can hide the equality-only memcmp use behind freeze on wasm and expand ordering
     * with expensive byte swaps. Word comparisons preserve equality without that lowering. */
    const unsigned char *x = (const unsigned char *)a;
    const unsigned char *y = (const unsigned char *)b;
    uint64_t diff = 0;
    for (; bytes >= 8; bytes -= 8, x += 8, y += 8) {
        uint64_t xa, ya;
        __builtin_memcpy(&xa, x, 8);
        __builtin_memcpy(&ya, y, 8);
        if (xa != ya) {
            return 0;
        }
    }
    for (; bytes; bytes--, x++, y++) {
        diff |= (uint64_t)(*x ^ *y);
    }
    return diff == 0;
#else
    const unsigned char *x = (const unsigned char *)a;
    const unsigned char *y = (const unsigned char *)b;
    if (bytes > 128) { return memcmp(a, b, bytes) == 0; }
    /* Small fixed comparisons avoid the wide integer reduction LLVM can emit for frozen
     * equality results. Overlapping end blocks stay within substring and span boundaries. */
    if (bytes >= 32) {
        if (__builtin_memcmp(x, y, 32) != 0) { return 0; }
        if (bytes > 64 && __builtin_memcmp(x + 32, y + 32, 32) != 0) { return 0; }
        if (bytes > 96 && __builtin_memcmp(x + 64, y + 64, 32) != 0) { return 0; }
        return __builtin_memcmp(x + bytes - 32, y + bytes - 32, 32) == 0;
    }
    if (bytes >= 16) {
        return __builtin_memcmp(x, y, 16) == 0 &&
               __builtin_memcmp(x + bytes - 16, y + bytes - 16, 16) == 0;
    }
    if (bytes >= 8) {
        return __builtin_memcmp(x, y, 8) == 0 &&
               __builtin_memcmp(x + bytes - 8, y + bytes - 8, 8) == 0;
    }
    if (bytes >= 4) {
        return __builtin_memcmp(x, y, 4) == 0 &&
               __builtin_memcmp(x + bytes - 4, y + bytes - 4, 4) == 0;
    }
    if (bytes >= 2) {
        return __builtin_memcmp(x, y, 2) == 0 &&
               __builtin_memcmp(x + bytes - 2, y + bytes - 2, 2) == 0;
    }
    return bytes == 0 || *x == *y;
#endif
}

#endif
