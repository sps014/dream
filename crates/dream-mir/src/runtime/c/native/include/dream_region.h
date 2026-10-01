#ifndef DREAM_REGION_H
#define DREAM_REGION_H

#include "dream_rt_native.h"

dream_ptr dream_region_try_malloc(dream_size size, int32_t tag);
int dream_region_owns(dream_ptr ptr);
dream_ptr dream_region_backing_malloc(dream_size size);
dream_ptr dream_region_activate(char *block, dream_size total, int32_t tag);
void dream_region_account_free(uint32_t count);
void dream_region_heap_mode(int active);

#ifdef DREAM_WASM32
int32_t dream_region_state_get(void);
void dream_region_state_set(int32_t value);
#endif

#endif
