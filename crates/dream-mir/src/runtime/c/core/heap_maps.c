#define _POSIX_C_SOURCE 200809L
#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_heap_maps.h"
#include <limits.h>

static _Noreturn void heap_map_oom(void) {
    DREAM_PANIC_LITERAL(u"out of memory indexing the Dream heap");
}

#include <stdlib.h>
#define realloc(ptr, size) dream_platform_current->resize(ptr, size)
#define free(ptr) dream_platform_current->deallocate(ptr)
#define utarray_oom() heap_map_oom()
#include "utarray.h"

typedef struct {
    uintptr_t base;
    size_t size;
} HeapMap;

static UT_array heap_maps = {.icd = {sizeof(HeapMap), NULL, NULL, NULL}};

static int compare_address(const void *key, const void *value) {
    const HeapMap *request = (const HeapMap *)key;
    const HeapMap *map = (const HeapMap *)value;
    if (request->base < map->base) {
        return -1;
    }
    uintptr_t offset = request->base - map->base;
    if (offset >= map->size || request->size > map->size - offset) {
        return 1;
    }
    return 0;
}

void dream_heap_map_add_locked(void *base, size_t size) {
    if (base == NULL || size == 0) {
        return;
    }
    size_t count = utarray_len(&heap_maps);
    if (count >= UINT_MAX / 2 || count >= SIZE_MAX / sizeof(HeapMap) / 2) {
        heap_map_oom();
    }
    HeapMap map = {(uintptr_t)base, size};
    /* Range lookup needs address order; insert the new mapping without hosted qsort. */
    unsigned position = 0;
    while (position < count && ((HeapMap *)utarray_eltptr(&heap_maps, position))->base < map.base) {
        ++position;
    }
    utarray_insert(&heap_maps, &map, position);
}

int dream_heap_map_contains_locked(const void *address, size_t size) {
    if (utarray_len(&heap_maps) == 0) {
        return 0;
    }
    HeapMap request = {(uintptr_t)address, size};
    size_t low = 0, high = utarray_len(&heap_maps);
    while (low < high) {
        size_t middle = low + (high - low) / 2;
        int order = compare_address(&request, utarray_eltptr(&heap_maps, middle));
        if (order == 0) { return 1; }
        if (order < 0) { high = middle; } else { low = middle + 1; }
    }
    return 0;
}

size_t dream_heap_map_count_locked(void) { return utarray_len(&heap_maps); }

void *dream_heap_map_at_locked(size_t index, size_t *size) {
    HeapMap *map = (HeapMap *)utarray_eltptr(&heap_maps, index);
    if (map == NULL) {
        *size = 0;
        return NULL;
    }
    *size = map->size;
    return (void *)map->base;
}
