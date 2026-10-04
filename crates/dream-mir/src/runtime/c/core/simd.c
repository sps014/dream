#include "dream_core.h"
#include "dream_platform_internal.h"

/* SIMD helpers are `static inline` in the header so guest -O3/LTO sees GCC
 * vector ops instead of TLS memcpy + a same-TU weak `return 0` stub. */
