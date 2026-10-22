#ifndef DREAM_WASM_HEAP_INTERNAL_H
#define DREAM_WASM_HEAP_INTERNAL_H
#include "dream_rt_wasm32.h"

extern int64_t total_allocations;
extern int64_t total_frees;
extern int64_t pinned_objects;
extern int32_t last_freed;
extern int32_t free_list_head;

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
dream_ptr dream_wasm_raw_malloc(int32_t size);
void dream_wasm_raw_free(dream_ptr ptr);

static inline int32_t size_class(int32_t size) {
    int32_t s = size;
    if (s < 16) {
        s = 16;
    }
    return 28 - __builtin_clz((unsigned)(s - 1));
}

static inline int32_t class_bytes(int32_t idx) {
    if (idx > 12) {
        return 0;
    }
    return 1 << (idx + 4);
}

#endif
