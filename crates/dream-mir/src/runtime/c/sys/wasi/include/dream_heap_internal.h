#ifndef DREAM_WASM_HEAP_INTERNAL_H
#define DREAM_WASM_HEAP_INTERNAL_H
#include "dream_rt_wasm32.h"

extern int64_t total_allocations;
extern int64_t total_frees;
extern int64_t pinned_objects;
extern int32_t last_freed;
extern int32_t free_list_head;

/* Per-class free lists and bump spans. Each instance owns one for private storage; the shared
 * copy in heap metadata serves locked allocations and receives an exiting instance's storage. */
typedef struct {
    int32_t free[DREAM_NCLASS];
    int32_t cursor[DREAM_NCLASS];
    int32_t end[DREAM_NCLASS];
} dream_class_lists;

/* Shared heap metadata beyond the data segments. The page map holds one byte per 64 KiB page
 * of linear memory: zero outside the managed heap, a span's class index plus one, or
 * DREAM_PAGE_LARGE for pages of first-fit large blocks. */
enum {
    META_HEAP_PTR = 0,
    META_TID = 4,
    META_LOCK = 8,
    META_WEAK_LOCK = 12,
    META_LARGE_HEAD = 16,
    META_CLASS_LISTS = 20,
    META_PAGE_MAP = (META_CLASS_LISTS + (int)sizeof(dream_class_lists) + 15) & -16,
    META_SIZE = META_PAGE_MAP + MAX_MEMORY_PAGES
};

enum { DREAM_PAGE_NONE = 0, DREAM_PAGE_LARGE = 0xff };

/* Large blocks keep `[size:i32][next:i32]` before the guest header; `size` counts from the
 * prefix so physically adjacent free blocks merge. */
enum { DREAM_LARGE_PREFIX = 8 };

int32_t dream_wasm_heap_start(void);
int32_t dream_wasm_heap_ptr_get(void);
int32_t dream_wasm_heap_claim(int32_t size);
int32_t dream_wasm_heap_claim_aligned(int32_t size, int32_t align, int32_t *gap);
int32_t *dream_wasm32_meta_i32(int32_t offset);
dream_ptr dream_wasm_raw_malloc(int32_t size);
void dream_wasm_raw_free(dream_ptr ptr);

#ifdef __wasm__
extern unsigned char __heap_base;
DREAM_ALWAYS_INLINE uint8_t *dream_wasm_page_map(void) {
    return (uint8_t *)((((uintptr_t)&__heap_base + 15u) & ~(uintptr_t)15u) + META_PAGE_MAP);
}
#else
DREAM_ALWAYS_INLINE uint8_t *dream_wasm_page_map(void) {
    return (uint8_t *)(uintptr_t)(uint32_t)(dream_wasm_heap_start() + META_PAGE_MAP);
}
#endif

/* Spans hold at least four blocks and are aligned to their own power-of-two size. */
DREAM_ALWAYS_INLINE int32_t dream_wasm_span_bytes(int idx) {
    uint32_t bytes = (uint32_t)dream_class_bytes(idx) * 4u;
    if (bytes <= WASM_PAGE_SIZE) {
        return (int32_t)WASM_PAGE_SIZE;
    }
    return (int32_t)(1u << (32 - __builtin_clz(bytes - 1u)));
}

DREAM_ALWAYS_INLINE unsigned dream_wasm_page_kind(int32_t address) {
    return dream_wasm_page_map()[(uint32_t)address >> 16];
}

#endif
