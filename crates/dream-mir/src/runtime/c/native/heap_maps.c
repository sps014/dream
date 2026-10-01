#define _POSIX_C_SOURCE 200809L
#include "include/dream_rt_native.h"
#include "include/dream_heap_maps.h"
#include <limits.h>
#include <stdlib.h>

static _Noreturn void heap_map_oom(void) {
    /* Reporting must not allocate from the Dream heap while its mutex is held. */
    static const struct {
        int32_t length;
        int32_t kind;
        uint16_t text[sizeof(u"out of memory indexing the Dream heap") / sizeof(uint16_t)];
    } message = {
        sizeof(u"out of memory indexing the Dream heap") / sizeof(uint16_t) - 1,
        DREAM_STR_PAD_INLINE,
        u"out of memory indexing the Dream heap"
    };
    dream_panic((dream_ptr)(uintptr_t)&message);
    __builtin_unreachable();
}

#define utarray_oom() heap_map_oom()
#include "include/utarray.h"

typedef struct {
    uintptr_t base;
    size_t size;
} HeapMap;

static UT_array heap_maps = {.icd = {sizeof(HeapMap), NULL, NULL, NULL}};

static int compare_maps(const void *left, const void *right) {
    uintptr_t a = ((const HeapMap *)left)->base;
    uintptr_t b = ((const HeapMap *)right)->base;
    return (a > b) - (a < b);
}

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
    utarray_push_back(&heap_maps, &map);
    utarray_sort(&heap_maps, compare_maps);
}

int dream_heap_map_contains_locked(const void *address, size_t size) {
    if (utarray_len(&heap_maps) == 0) {
        return 0;
    }
    HeapMap request = {(uintptr_t)address, size};
    return utarray_find(&heap_maps, &request, compare_address) != NULL;
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
