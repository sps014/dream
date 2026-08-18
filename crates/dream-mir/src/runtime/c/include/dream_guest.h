#ifndef DREAM_GUEST_H
#define DREAM_GUEST_H

#ifdef DREAM_NATIVE
#include "dream_rt_native.h"

#define dream_alloc dream_malloc
#define dream_drop dream_release

static inline int32_t dream_array_len(dream_ptr p) {
    return p ? dream_i32(p)[0] : 0;
}

static inline int32_t *dream_i32s(dream_ptr p) {
    return dream_i32(p);
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

#else
#include "dream_rt.h"

typedef int32_t dream_ptr;

IMPORT("malloc") int32_t malloc_tagged(int32_t size, int32_t tag);
IMPORT("free") void free_tagged(int32_t ptr);

static inline dream_ptr dream_alloc(int32_t size, int32_t tag) {
    return malloc_tagged(size, tag);
}

static inline void dream_free(dream_ptr ptr) {
    free_tagged(ptr);
}

#define dream_drop dream_free

static inline int32_t dream_array_len(dream_ptr p) {
    return p ? i32_load(p) : 0;
}

static inline int32_t *dream_i32s(dream_ptr p) {
    return (int32_t *)(uintptr_t)(uint32_t)p;
}

static inline int32_t dream_str_len(dream_ptr s) {
    return s ? i32_load(s) : 0;
}

static inline const uint16_t *dream_str_units(dream_ptr s) {
    int32_t d;
    if (s == 0) {
        return NULL;
    }
    d = i32_load(s + (int32_t)STRING_SCALAR_LEN_OFFSET);
    if (d == DREAM_STR_PAD_INLINE) {
        return (const uint16_t *)(uintptr_t)(uint32_t)(s + (int32_t)STRING_UNITS_OFFSET);
    }
    return (const uint16_t *)(uintptr_t)(uint32_t)d;
}

static inline dream_ptr dream_str_from_units(const uint16_t *u, int32_t n) {
    dream_ptr p;
    if (n <= 0 || u == NULL) {
        return intern_empty();
    }
    p = dream_alloc(n * 2 + (int32_t)STRING_HEADER_SIZE, TAG_STRING);
    i32_store(p, n);
    i32_store(p + (int32_t)STRING_SCALAR_LEN_OFFSET, p + (int32_t)STRING_UNITS_OFFSET);
    mem_copy(p + (int32_t)STRING_UNITS_OFFSET, (int32_t)(uintptr_t)u, n * 2);
    return p;
}

static inline dream_ptr dream_array_i32(int32_t count) {
    dream_ptr p = dream_alloc((int32_t)LEN_PREFIX_SIZE + count * 4, TAG_ARRAY);
    int32_t i;
    i32_store(p, count);
    for (i = 0; i < count; i++) {
        i32_store(p + (int32_t)LEN_PREFIX_SIZE + i * 4, 0);
    }
    return p;
}

#endif

#endif
