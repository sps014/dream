#ifndef DREAM_REGION_H
#define DREAM_REGION_H

#include "dream_core.h"

dream_ptr dream_region_try_malloc(dream_size size, int32_t tag);
int dream_region_owns(dream_ptr ptr);
dream_ptr dream_region_backing_malloc(dream_size size);
/* Exact edge metadata is retained for publication of references outside a region. */
dream_ptr dream_region_activate(char *block, dream_size total, int32_t tag, const dream_type_info *info);
void dream_region_account_free(uint32_t count);
void dream_region_heap_mode(int active);

/* Bytes a region block of `total` header-plus-payload bytes occupies. Native headers derive a
 * block's size from its class, so region blocks fill a whole class or carry the large prefix. */
static inline size_t dream_region_block_bytes(size_t total) {
#ifdef DREAM_WASM32
    return total;
#else
    if (total <= 256u) {
        return total;
    }
    return total > DREAM_MAX_CLASS_BYTES ? total + DREAM_LARGE_PREFIX
                                         : (size_t)dream_class_bytes(dream_size_class((uint32_t)total));
#endif
}

#ifdef DREAM_WASM32
int32_t dream_region_state_get(void);
void dream_region_state_set(int32_t value);
#endif

#endif
