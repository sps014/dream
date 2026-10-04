#include "dream_core.h"
#include "dream_platform_internal.h"

void dream_ffi_free_with(uintptr_t free_fn, uintptr_t ptr) {
    if (free_fn && ptr) {
        ((void (*)(void *))free_fn)((void *)ptr);
    }
}

uintptr_t dream_ffi_read_ptr(uintptr_t base, int32_t index) {
    if (!base) {
        return 0;
    }
    return (uintptr_t)((void **)base)[index];
}

int32_t dream_ffi_read_i32(uintptr_t base, int32_t index) {
    return ((const int32_t *)(uintptr_t)base)[index];
}

int64_t dream_ffi_read_i64(uintptr_t base, int32_t index) {
    return ((const int64_t *)(uintptr_t)base)[index];
}

double dream_ffi_read_f64(uintptr_t base, int32_t index) {
    return ((const double *)(uintptr_t)base)[index];
}

dream_ptr dream_ffi_read_cstring(uintptr_t ptr) {
    if (!ptr) {
        return dream_string_alloc(0);
    }
    return dream_utf8_to_string((const char *)(uintptr_t)ptr);
}
