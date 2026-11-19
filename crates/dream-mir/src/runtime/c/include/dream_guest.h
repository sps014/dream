#ifndef DREAM_GUEST_H
#define DREAM_GUEST_H

#if !defined(DREAM_NATIVE) && !defined(DREAM_WASM32)
#define DREAM_WASM32 1
#endif
#include "dream_core.h"

static inline int32_t dream_array_len(dream_ptr p) {
    return p ? dream_i32(p)[0] : 0;
}

static inline int32_t *dream_array_i32s(dream_ptr p) {
    return (int32_t *)dream_array_bytes(p);
}

static inline dream_ptr dream_str_from_units(const uint16_t *u, int32_t n) {
    dream_ptr p;
    if (n <= 0 || u == NULL) {
        return dream_string_alloc(0);
    }
    p = dream_string_alloc(n);
    memcpy((char *)dream_p(p) + STRING_UNITS_OFFSET, u, (size_t)n * 2);
    return p;
}

static inline dream_ptr dream_array_i32(int32_t count) {
    return dream_array_new(count, 4);
}

#endif
