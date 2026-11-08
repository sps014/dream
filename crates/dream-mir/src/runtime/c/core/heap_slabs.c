#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_heap_maps.h"

/* The injected platform promises 16-byte alignment. Overmapping keeps slab alignment independent
 * of OS page size and custom platform implementations; gauges include the alignment reservation. */
static dream_heap_span *map_span(unsigned cls, size_t bytes, int locked) {
    if (bytes > PTRDIFF_MAX - DREAM_SLAB_BYTES) {
        DREAM_PANIC_LITERAL(u"panic: allocation size exceeds the supported limit");
    }
    size_t mapped = bytes + DREAM_SLAB_BYTES - 1u;
    char *raw = dream_platform_current->map(mapped);
    if (!raw) {
        if (locked) { DREAM_PANIC_LITERAL(u"panic: out of memory mapping the shared heap"); }
        DREAM_PANIC_LITERAL(u"panic: out of memory mapping the private heap");
    }
    uintptr_t aligned = ((uintptr_t)raw + DREAM_SLAB_BYTES - 1u) & ~(uintptr_t)(DREAM_SLAB_BYTES - 1u);
    dream_heap_span *span = (dream_heap_span *)aligned;
    DREAM_POISON((char *)span + DREAM_SLAB_PREFIX, bytes - DREAM_SLAB_PREFIX);
    *span = (dream_heap_span){.cls = cls, .block_bytes = cls < DREAM_NCLASS ? (size_t)dream_class_bytes((int)cls) : 0};
    if (!locked) { dream_platform_current->lock(DREAM_LOCK_HEAP); }
    dream_heap_map_add_locked(span, bytes, mapped);
    if (!locked) { dream_platform_current->unlock(DREAM_LOCK_HEAP); }
    return span;
}

char *dream_slab_bump(dream_heap_span **current, unsigned cls, int locked) {
    dream_heap_span *span = *current;
    size_t bytes = (size_t)dream_class_bytes((int)cls);
    if (!span || span->used >= (DREAM_SLAB_BYTES - DREAM_SLAB_PREFIX) / bytes) {
        span = map_span(cls, DREAM_SLAB_BYTES, locked);
        *current = span;
    }
    uint32_t used = __atomic_load_n(&span->used, __ATOMIC_RELAXED);
    char *block = (char *)span + DREAM_SLAB_PREFIX + (size_t)used * bytes;
    DREAM_UNPOISON(block, HEAP_HEADER_SIZE);
    dream_heap_header *header = (dream_heap_header *)block;
    header->tag = DREAM_TAG_FREED;
    header->rc = 0;
    __atomic_store_n(&span->used, used + 1u, __ATOMIC_RELEASE);
    return block;
}

char *dream_large_map(size_t total, int locked) {
    if (total > PTRDIFF_MAX - DREAM_SLAB_PREFIX - DREAM_LARGE_PREFIX) {
        DREAM_PANIC_LITERAL(u"panic: allocation size exceeds the supported limit");
    }
    dream_heap_span *span = map_span(DREAM_CLASS_LARGE, DREAM_SLAB_PREFIX + DREAM_LARGE_PREFIX + total, locked);
    char *block = (char *)span + DREAM_SLAB_PREFIX + DREAM_LARGE_PREFIX;
    DREAM_UNPOISON(block - DREAM_LARGE_PREFIX, DREAM_LARGE_PREFIX + HEAP_HEADER_SIZE);
    *dream_large_size(block) = total;
    *dream_large_state(block) = 0;
    dream_heap_header *header = (dream_heap_header *)block;
    header->tag = DREAM_TAG_FREED;
    header->rc = 0;
    __atomic_store_n(&span->used, 1u, __ATOMIC_RELEASE);
    return block;
}
