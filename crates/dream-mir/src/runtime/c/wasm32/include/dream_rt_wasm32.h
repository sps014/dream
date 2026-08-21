#ifndef DREAM_RT_WASM32_H
#define DREAM_RT_WASM32_H

#ifndef DREAM_WASM32
#define DREAM_WASM32 1
#endif
#include "../../native/include/dream_rt_native.h"

/* Heap-meta lock slot for the weak-reference registry (see weak_stub.c). */
#define DREAM_META_WEAK_LOCK 72
int32_t *dream_wasm32_meta_i32(int32_t off);

#endif
