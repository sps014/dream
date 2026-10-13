#include "dream_core.h"
#include "dream_platform_internal.h"

typedef struct {
    void *base;
    size_t size;
    size_t alignment;
} RawAllocation;

typedef struct ArenaChunk {
    struct ArenaChunk *next;
    size_t capacity;
    size_t used;
} ArenaChunk;

typedef struct {
    ArenaChunk *first;
    ArenaChunk *current;
    ArenaChunk *last;
} ArenaState;

static size_t add_size(size_t a, size_t b) {
    if (a > SIZE_MAX - b) { DREAM_PANIC_LITERAL(u"raw memory: size overflow"); }
    return a + b;
}

size_t dream_raw_size(size_t count, size_t element_size) {
    if (element_size && count > SIZE_MAX / element_size) {
        DREAM_PANIC_LITERAL(u"raw memory: size overflow");
    }
    return count * element_size;
}

static void check_alignment(size_t alignment) {
    if (!alignment || (alignment & (alignment - 1))) {
        DREAM_PANIC_LITERAL(u"raw memory: alignment must be a nonzero power of two");
    }
}

static void *allocate(size_t size) {
    void *p = dream_platform_current->allocate(size);
    if (!p) { DREAM_PANIC_LITERAL(u"raw memory: allocation failed"); }
    return p;
}

uintptr_t dream_raw_alloc(size_t size, size_t alignment, size_t minimum, int32_t zeroed) {
    check_alignment(alignment);
    if (alignment < minimum) { DREAM_PANIC_LITERAL(u"raw memory: insufficient alignment"); }
    if (!size) { return 0; }
    if (alignment < _Alignof(RawAllocation)) { alignment = _Alignof(RawAllocation); }
    size_t total = add_size(add_size(size, sizeof(RawAllocation)), alignment - 1);
    void *base = allocate(total);
    uintptr_t start = (uintptr_t)base + sizeof(RawAllocation);
    uintptr_t address = (start + alignment - 1) & ~(uintptr_t)(alignment - 1);
    RawAllocation *meta = ((RawAllocation *)address) - 1;
    *meta = (RawAllocation){base, size, alignment};
    if (zeroed) { memset((void *)address, 0, size); }
    return address;
}

void dream_raw_free(uintptr_t address) {
    if (address) {
        RawAllocation *meta = ((RawAllocation *)address) - 1;
        dream_platform_current->deallocate(meta->base);
    }
}

uintptr_t dream_raw_realloc(uintptr_t address, size_t size, size_t minimum) {
    if (!size) { dream_raw_free(address); return 0; }
    if (!address) { return dream_raw_alloc(size, minimum, minimum, 0); }
    RawAllocation *meta = ((RawAllocation *)address) - 1;
    size_t alignment = meta->alignment < minimum ? minimum : meta->alignment;
    uintptr_t next = dream_raw_alloc(size, alignment, minimum, 0);
    memcpy((void *)next, (void *)address, size < meta->size ? size : meta->size);
    dream_raw_free(address);
    return next;
}

void dream_raw_copy(uintptr_t dst, uintptr_t src, size_t size) {
    if (size) { memcpy((void *)dst, (void *)src, size); }
}

void dream_raw_move(uintptr_t dst, uintptr_t src, size_t size) {
    if (size) { memmove((void *)dst, (void *)src, size); }
}

void dream_raw_fill(uintptr_t dst, int32_t value, size_t size) {
    if (size) { memset((void *)dst, value, size); }
}

static ArenaChunk *new_chunk(size_t capacity) {
    ArenaChunk *chunk = allocate(add_size(sizeof(ArenaChunk), capacity));
    *chunk = (ArenaChunk){NULL, capacity, 0};
    return chunk;
}

uintptr_t dream_arena_new(size_t initial) {
    ArenaState *state = allocate(sizeof(ArenaState));
    if (!initial) { initial = 1; }
    ArenaChunk *chunk = new_chunk(initial);
    *state = (ArenaState){chunk, chunk, chunk};
    return (uintptr_t)state;
}

static ArenaState *arena_state(uintptr_t address) {
    if (!address) { DREAM_PANIC_LITERAL(u"Arena: handle is not initialized"); }
    return (ArenaState *)address;
}

static uintptr_t chunk_alloc(ArenaChunk *chunk, size_t size, size_t alignment) {
    uintptr_t base = (uintptr_t)(chunk + 1);
    uintptr_t at = base + chunk->used;
    size_t padding = (alignment - (at & (alignment - 1))) & (alignment - 1);
    size_t available = chunk->capacity - chunk->used;
    if (padding > available || size > available - padding) { return 0; }
    chunk->used += padding + size;
    return at + padding;
}

uintptr_t dream_arena_alloc(uintptr_t address, size_t size, size_t alignment, int32_t zeroed) {
    ArenaState *state = arena_state(address);
    check_alignment(alignment);
    if (!size) { return 0; }
    ArenaChunk *chunk = state->current;
    uintptr_t result;
    while ((result = chunk_alloc(chunk, size, alignment)) == 0) {
        if (chunk->next) { chunk = chunk->next; continue; }
        size_t doubled = add_size(state->last->capacity, state->last->capacity);
        size_t required = add_size(size, alignment - 1);
        chunk = new_chunk(doubled > required ? doubled : required);
        state->last->next = chunk;
        state->last = chunk;
    }
    state->current = chunk;
    if (zeroed) { memset((void *)result, 0, size); }
    return result;
}

void dream_arena_reset(uintptr_t address) {
    ArenaState *state = arena_state(address);
    for (ArenaChunk *chunk = state->first; chunk; chunk = chunk->next) { chunk->used = 0; }
    state->current = state->first;
}

void dream_arena_free(uintptr_t address) {
    if (!address) { return; }
    ArenaState *state = (ArenaState *)address;
    ArenaChunk *chunk = state->first;
    while (chunk) {
        ArenaChunk *next = chunk->next;
        dream_platform_current->deallocate(chunk);
        chunk = next;
    }
    dream_platform_current->deallocate(state);
}
