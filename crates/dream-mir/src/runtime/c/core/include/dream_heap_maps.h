#ifndef DREAM_HEAP_MAPS_H
#define DREAM_HEAP_MAPS_H

#include <stddef.h>
#include <stdint.h>

/* The allocator's heap mutex protects both growth and readers of this index. */
void dream_heap_map_add_locked(void *base, size_t size);
int dream_heap_map_contains_locked(const void *address, size_t size);
size_t dream_heap_map_count_locked(void);
void *dream_heap_map_at_locked(size_t index, size_t *size);
/* Bytes of free large blocks whose pages are currently handed back to the OS; takes the lock. */
uint64_t dream_heap_released_bytes(void);

#endif
