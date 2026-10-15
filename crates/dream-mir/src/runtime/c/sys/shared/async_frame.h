#ifndef DREAM_ASYNC_FRAME_H
#define DREAM_ASYNC_FRAME_H
#include "dream_core.h"

static inline int32_t *i32_at(dream_ptr p, int32_t off) {
    return (int32_t *)((char *)dream_p(p) + off);
}

static inline dream_ptr *ptr_at(dream_ptr p, int32_t off) {
    return (dream_ptr *)((char *)dream_p(p) + off);
}

static inline dream_result *result_at(dream_ptr p) {
    return (dream_result *)((char *)dream_p(p) + F_RESULT);
}

/* The 4-byte length prefix leaves native 8-byte elements only 4-aligned. */
static inline dream_ptr arr_get(dream_ptr arr, int32_t i) {
    dream_ptr element;
    memcpy(&element, (char *)dream_p(arr) + LEN_PREFIX_SIZE + (size_t)i * sizeof(dream_ptr), sizeof(element));
    return element;
}

/* Native completion publishes the result from another thread; every status access
 * must be atomic, including scheduler fast paths that only test readiness. */
static inline int32_t status_of(dream_ptr f) {
#ifdef DREAM_WASM32
    return i32_at(f, F_STATUS)[0];
#else
    return __atomic_load_n(i32_at(f, F_STATUS), __ATOMIC_ACQUIRE);
#endif
}

static inline void status_store(dream_ptr f, int32_t status) {
#ifdef DREAM_WASM32
    i32_at(f, F_STATUS)[0] = status;
#else
    __atomic_store_n(i32_at(f, F_STATUS), status, __ATOMIC_RELEASE);
#endif
}

void dream_combinator_progress(dream_ptr parent, dream_ptr child);
#endif
