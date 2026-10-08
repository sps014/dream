#include "dream_region.h"

#define REGION_MAX_DEPTH 8u
#define REGION_PAYLOAD (4u << 20)

typedef struct region_chunk {
    struct region_chunk *previous;
    size_t capacity;
    size_t offset;
} region_chunk;

typedef struct {
    region_chunk *chunk;
    size_t offset;
    uint32_t allocations;
} region_mark;

typedef struct {
    uint32_t depth;
    uint32_t allocations;
    region_chunk *chunk;
    region_mark marks[REGION_MAX_DEPTH];
} region_state;

#ifndef DREAM_WASM32
static _Thread_local region_state state;
#endif

static region_state *current(void) {
#ifdef DREAM_WASM32
    return (region_state *)dream_p((dream_ptr)dream_region_state_get());
#else
    return &state;
#endif
}

static void region_panic(void) {
    DREAM_PANIC_LITERAL(u"panic: region allocation exceeds the supported object size");
}

void dream_region_enter(void) {
    region_state *s = current();
#ifdef DREAM_WASM32
    if (s == NULL) {
        dream_ptr pointer = dream_region_backing_malloc((int32_t)sizeof(*s));
        s = (region_state *)dream_p(pointer);
        memset(s, 0, sizeof(*s));
        dream_region_state_set(pointer);
    }
#endif
    if (s->depth == UINT32_MAX) {
        region_panic();
        return;
    }
    if (s->depth < REGION_MAX_DEPTH) {
        region_mark *mark = &s->marks[s->depth];
        mark->chunk = s->chunk;
        mark->offset = s->chunk == NULL ? 0 : s->chunk->offset;
        mark->allocations = s->allocations;
    }
    ++s->depth;
    dream_region_heap_mode(1);
}

static region_state *active_region(void) {
    region_state *s = current();
    return s == NULL || s->depth == 0 || s->depth > REGION_MAX_DEPTH ? NULL : s;
}

int dream_region_active(void) { return active_region() != NULL; }

static dream_ptr region_bump(region_state *s, dream_size size, int32_t tag, const dream_type_info *info) {
    if ((size_t)size > DREAM_SIZE_MAX - 63 - sizeof(region_chunk)) {
        region_panic();
        return 0;
    }
    size_t total = ((size_t)size + DREAM_BLOCK_HEADER + 15u) & ~(size_t)15u;
    region_chunk *chunk = s->chunk;
    if (chunk == NULL || total > chunk->capacity - chunk->offset) {
        size_t prefix = (sizeof(region_chunk) + 15u) & ~(size_t)15u;
        size_t capacity = prefix + (total > REGION_PAYLOAD ? total : REGION_PAYLOAD);
        dream_ptr pointer = dream_region_backing_malloc((dream_size)capacity);
        chunk = (region_chunk *)dream_p(pointer);
        chunk->previous = s->chunk;
        chunk->capacity = capacity;
        chunk->offset = prefix;
        s->chunk = chunk;
    }
    char *block = (char *)chunk + chunk->offset;
    chunk->offset += total;
    ++s->allocations;
    return dream_region_activate(block, (dream_size)total, tag, info);
}

dream_ptr dream_region_try_malloc(dream_size size, int32_t tag) {
    region_state *s = active_region();
    if (s == NULL || (tag & TAG_SHARED)) {
        return 0;
    }
    const dream_type_info *info = dream_type_info_for_tag(tag & TAG_VALUE_MASK);
    /* Bulk reclamation would strand a collector registration; only `dream_malloc_private`
     * may place a cycle-capable type in a region. */
    if (info && info->cycle_capable) {
        return 0;
    }
    return region_bump(s, size, tag, info);
}

dream_ptr dream_region_try_malloc_private(dream_size size, int32_t tag, const dream_type_info *untracked) {
    region_state *s = active_region();
    return s == NULL ? 0 : region_bump(s, size, tag, untracked);
}

int dream_region_owns(dream_ptr pointer) {
    region_state *s = current();
    if (s == NULL || s->depth == 0) {
        return 0;
    }
    uintptr_t address = (uintptr_t)dream_p(pointer);
    for (region_chunk *chunk = s->chunk; chunk != NULL; chunk = chunk->previous) {
        uintptr_t base = (uintptr_t)chunk;
        if (address >= base && address - base < chunk->offset) {
            return 1;
        }
    }
    return 0;
}

void dream_region_leave(void) {
    region_state *s = current();
    if (s == NULL || s->depth == 0) {
        return;
    }
    if (--s->depth >= REGION_MAX_DEPTH) {
        return;
    }
    region_mark mark = s->marks[s->depth];
    dream_region_account_free(s->allocations - mark.allocations);
    /* Leave is rare next to allocation, so the shared count is updated here rather than per object. */
#if defined(DREAM_RUNTIME_COUNTERS) && DREAM_RUNTIME_COUNTERS
    __atomic_fetch_add(&dream_runtime_counters[DREAM_COUNT_REGION_OBJECTS],
                       (uint64_t)(s->allocations - mark.allocations), __ATOMIC_RELAXED);
#endif
    s->allocations = mark.allocations;
    while (s->chunk != mark.chunk) {
        region_chunk *chunk = s->chunk;
        s->chunk = chunk->previous;
        /* Detach before recycling so the backing block is not mistaken for a region object. */
        dream_recycle((dream_ptr)chunk);
    }
    if (s->chunk != NULL) {
        s->chunk->offset = mark.offset;
    }
    if (s->depth == 0) {
#ifdef DREAM_WASM32
        dream_region_state_set(0);
        dream_recycle((dream_ptr)s);
#endif
        dream_region_heap_mode(0);
    }
}
