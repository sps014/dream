#include "include/dream_rt_native.h"

int64_t dream_ffi_read_ptr(int64_t base, int32_t index) {
    if (!base) {
        return 0;
    }
    return (int64_t)(uintptr_t)((void **)(uintptr_t)base)[index];
}

dream_ptr dream_ffi_read_cstring(int64_t ptr) {
    if (!ptr) {
        return dream_string_alloc(0);
    }
    return dream_utf8_to_string((const char *)(uintptr_t)ptr);
}
