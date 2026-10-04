#include "dream_core.h"
#include "dream_platform_internal.h"
const dream_platform *dream_platform_current = &dream_default_platform;
void dream_set_platform(const dream_platform *platform) {
    dream_platform_current = platform;
}
void *dream_raw_calloc(size_t count, size_t size) {
    if (size && count > SIZE_MAX / size) { return NULL; }
    size_t bytes = count * size;
    void *p = dream_platform_current->allocate(bytes);
    if (p) { memset(p, 0, bytes); }
    return p;
}
_Noreturn void dream_platform_abort(void) {
    dream_platform_current->abort();
    __builtin_trap();
}
