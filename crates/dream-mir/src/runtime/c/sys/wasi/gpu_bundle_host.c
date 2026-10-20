#include "dream_gpu_bundle.h"
#include <stddef.h>

extern uint8_t __heap_base;
static uint32_t cursor;

void gpu_bundle_reset(void) { cursor = (uint32_t)(uintptr_t)&__heap_base; }

uint32_t gpu_bundle_alloc(uint32_t length) {
    if (!cursor) gpu_bundle_reset();
    if (length > 64 * 1024 * 1024 || cursor > UINT32_MAX - length - 15) return 0;
    uint32_t start = (cursor + 15) & ~15u, end = start + length;
    uint32_t pages = __builtin_wasm_memory_size(0);
    if (end > pages * 65536u) {
        uint32_t extra = (end - pages * 65536u + 65535u) / 65536u;
        if (__builtin_wasm_memory_grow(0, extra) == (size_t)-1) return 0;
    }
    cursor = end;
    return start;
}
