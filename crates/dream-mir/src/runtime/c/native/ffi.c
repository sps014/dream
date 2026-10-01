#include "include/dream_rt_native.h"

#include <stdio.h>
#include <stdlib.h>

int64_t dream_ffi_read_ptr(int64_t base, int32_t index) {
    if (!base) {
        return 0;
    }
    return (int64_t)(uintptr_t)((void **)(uintptr_t)base)[index];
}

int32_t dream_ffi_read_i32(int64_t base, int32_t index) {
    return ((const int32_t *)(uintptr_t)base)[index];
}

int64_t dream_ffi_read_i64(int64_t base, int32_t index) {
    return ((const int64_t *)(uintptr_t)base)[index];
}

double dream_ffi_read_f64(int64_t base, int32_t index) {
    return ((const double *)(uintptr_t)base)[index];
}

dream_ptr dream_ffi_read_cstring(int64_t ptr) {
    if (!ptr) {
        return dream_string_alloc(0);
    }
    return dream_utf8_to_string((const char *)(uintptr_t)ptr);
}
