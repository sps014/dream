#ifndef DREAM_GPU_BUNDLE_H
#define DREAM_GPU_BUNDLE_H
#include <stdint.h>
#define DREAM_GPU_BUNDLE_RECORD_WORDS 16
/* Offsets refer to the original immutable bundle, never host pointers. An absent output
 * counts records without allocating. A negative result is an error, not a partial table. */
uint32_t dream_gpu_bundle_abi_version(void);
int32_t dream_gpu_bundle_index(const uint8_t *bytes, uint32_t length,
                              uint32_t *records, uint32_t capacity);
const char *dream_gpu_bundle_error(int32_t status);
#endif
