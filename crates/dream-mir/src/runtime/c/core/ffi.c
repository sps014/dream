#include "dream_core.h"
#include "dream_platform_internal.h"

#include <stdint.h>
#include <string.h>

void dream_ffi_free_with(uintptr_t free_fn, uintptr_t ptr) {
    if (free_fn && ptr) {
        ((void (*)(void *))free_fn)((void *)ptr);
    }
}

static void ffi_base(uintptr_t base) {
    if (!base) {
        DREAM_PANIC_LITERAL(u"Ffi: null pointer");
    }
}

static uintptr_t ffi_at(uintptr_t base, intptr_t index, size_t size) {
    ffi_base(base);
    return (uintptr_t)((intptr_t)base + index * (intptr_t)size);
}

uintptr_t dream_ffi_read_ptr(uintptr_t base, intptr_t index) {
    return *(uintptr_t *)ffi_at(base, index, sizeof(uintptr_t));
}

uint8_t dream_ffi_read_u8(uintptr_t base, intptr_t index) {
    return *(uint8_t *)ffi_at(base, index, 1);
}

int32_t dream_ffi_read_i32(uintptr_t base, intptr_t index) {
    return *(int32_t *)ffi_at(base, index, 4);
}

uint32_t dream_ffi_read_u32(uintptr_t base, intptr_t index) {
    return *(uint32_t *)ffi_at(base, index, 4);
}

int64_t dream_ffi_read_i64(uintptr_t base, intptr_t index) {
    return *(int64_t *)ffi_at(base, index, 8);
}

uint64_t dream_ffi_read_u64(uintptr_t base, intptr_t index) {
    return *(uint64_t *)ffi_at(base, index, 8);
}

float dream_ffi_read_f32(uintptr_t base, intptr_t index) {
    return *(float *)ffi_at(base, index, 4);
}

double dream_ffi_read_f64(uintptr_t base, intptr_t index) {
    return *(double *)ffi_at(base, index, 8);
}

intptr_t dream_ffi_read_isize(uintptr_t base, intptr_t index) {
    return *(intptr_t *)ffi_at(base, index, sizeof(intptr_t));
}

uintptr_t dream_ffi_read_usize(uintptr_t base, intptr_t index) {
    return *(uintptr_t *)ffi_at(base, index, sizeof(uintptr_t));
}

void dream_ffi_write_ptr(uintptr_t base, intptr_t index, uintptr_t value) {
    *(uintptr_t *)ffi_at(base, index, sizeof(uintptr_t)) = value;
}

void dream_ffi_write_u8(uintptr_t base, intptr_t index, uint8_t value) {
    *(uint8_t *)ffi_at(base, index, 1) = value;
}

void dream_ffi_write_i32(uintptr_t base, intptr_t index, int32_t value) {
    *(int32_t *)ffi_at(base, index, 4) = value;
}

void dream_ffi_write_u32(uintptr_t base, intptr_t index, uint32_t value) {
    *(uint32_t *)ffi_at(base, index, 4) = value;
}

void dream_ffi_write_i64(uintptr_t base, intptr_t index, int64_t value) {
    *(int64_t *)ffi_at(base, index, 8) = value;
}

void dream_ffi_write_u64(uintptr_t base, intptr_t index, uint64_t value) {
    *(uint64_t *)ffi_at(base, index, 8) = value;
}

void dream_ffi_write_f32(uintptr_t base, intptr_t index, float value) {
    *(float *)ffi_at(base, index, 4) = value;
}

void dream_ffi_write_f64(uintptr_t base, intptr_t index, double value) {
    *(double *)ffi_at(base, index, 8) = value;
}

void dream_ffi_write_isize(uintptr_t base, intptr_t index, intptr_t value) {
    *(intptr_t *)ffi_at(base, index, sizeof(intptr_t)) = value;
}

void dream_ffi_write_usize(uintptr_t base, intptr_t index, uintptr_t value) {
    *(uintptr_t *)ffi_at(base, index, sizeof(uintptr_t)) = value;
}

dream_ptr dream_ffi_read_bytes(uintptr_t ptr, intptr_t len) {
    dream_ptr array;
    ffi_base(ptr);
    if (len < 0 || (uint64_t)len > (uint64_t)INT32_MAX) {
        DREAM_PANIC_LITERAL(u"Ffi: length is out of range");
    }
    array = dream_array_new((int32_t)len, 1);
    if (len > 0) {
        memcpy(dream_array_bytes(array), (const void *)ptr, (size_t)len);
    }
    return array;
}

dream_ptr dream_ffi_read_cstring(uintptr_t ptr) {
    if (!ptr) {
        return dream_string_alloc(0);
    }
    return dream_utf8_to_string((const char *)(uintptr_t)ptr);
}

dream_ptr dream_ffi_read_cstring_n(uintptr_t ptr, intptr_t max) {
    size_t n = 0;
    const char *s;
    char *buf;
    dream_ptr out;
    if (!ptr) {
        return dream_string_alloc(0);
    }
    if (max < 0) {
        DREAM_PANIC_LITERAL(u"Ffi: negative length");
    }
    s = (const char *)(uintptr_t)ptr;
    while (n < (size_t)max && s[n] != 0) {
        n++;
    }
    buf = (char *)dream_platform_current->allocate(n + 1);
    if (!buf) {
        DREAM_PANIC_LITERAL(u"Ffi: out of memory");
    }
    memcpy(buf, s, n);
    buf[n] = 0;
    out = dream_utf8_to_string(buf);
    dream_platform_current->deallocate(buf);
    return out;
}
