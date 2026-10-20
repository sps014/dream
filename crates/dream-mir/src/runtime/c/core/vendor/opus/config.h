/* Dream's portable scalar build; no target-specific SIMD or hosted allocator state. */
#ifndef DREAM_OPUS_CONFIG_H
#define DREAM_OPUS_CONFIG_H
#include "dream_platform_internal.h"
#define OPUS_BUILD 1
#define VAR_ARRAYS 1
#define HAVE_LRINT 1
#define HAVE_LRINTF 1
#define PACKAGE_VERSION "1.5.2"
#define PACKAGE_NAME "opus"
#define OVERRIDE_OPUS_ALLOC 1
#define OVERRIDE_OPUS_REALLOC 1
#define OVERRIDE_OPUS_FREE 1
#define opus_alloc(n) dream_platform_current->allocate(n)
#define opus_realloc(p, n) dream_platform_current->resize(p, n)
#define opus_free(p) dream_platform_current->deallocate(p)
#endif
