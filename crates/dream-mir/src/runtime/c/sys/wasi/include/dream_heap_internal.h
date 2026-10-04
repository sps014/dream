#ifndef DREAM_WASM_HEAP_INTERNAL_H
#define DREAM_WASM_HEAP_INTERNAL_H
#include "dream_rt_wasm32.h"

enum {
    META_HEAP_PTR = 0,
    META_TID = 4,
    META_LOCK = 8,
    META_FL = 12,
    META_WEAK_LOCK = 72,
    META_SIZE = 80
};

int32_t dream_wasm_heap_start(void);
int32_t dream_wasm_heap_ptr_get(void);
int32_t dream_wasm_heap_claim(int32_t size);
int32_t *dream_wasm32_meta_i32(int32_t offset);
#endif
