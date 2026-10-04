#ifndef DREAM_PLATFORM_INTERNAL_H
#define DREAM_PLATFORM_INTERNAL_H
#include <stdint.h>
#include "../../include/dream_platform.h"
extern const dream_platform dream_default_platform;
extern const dream_platform *dream_platform_current;
void dream_set_platform(const dream_platform *platform);
void *dream_raw_calloc(size_t count, size_t size);
_Noreturn void dream_platform_abort(void);
int32_t dream_utf8_chunk(const uint16_t *units, int32_t len, char *out, size_t cap, int32_t start, size_t *written);
void dream_write_utf16(int stream, const uint16_t *units, int32_t len);
#endif
