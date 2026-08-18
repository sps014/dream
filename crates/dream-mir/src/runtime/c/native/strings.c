#include "include/dream_rt_native.h"

dream_ptr dream_string_alloc(int32_t units) {
    dream_ptr p;
    if (units < 0) {
        units = 0;
    }
    p = dream_malloc((int32_t)((size_t)units * 2 + 8), TAG_STRING);
    dream_i32(p)[0] = units;
    dream_str_init_owned(p);
    return p;
}

dream_ptr dream_array_new(int32_t len, int32_t esize) {
    int32_t size;
    dream_ptr p;
    if (len < 0) {
        len = 0;
    }
    size = 4 + len * esize;
    p = dream_malloc(size, TAG_ARRAY);
    memset(dream_p(p), 0, (size_t)size);
    dream_i32(p)[0] = len;
    return p;
}

dream_ptr dream_array_realloc(dream_ptr arr, int32_t new_len, int32_t esize) {
    int32_t old_len = arr ? dream_i32(arr)[0] : 0;
    dream_ptr p = dream_realloc(arr, 4 + new_len * esize, TAG_ARRAY);
    dream_i32(p)[0] = new_len;
    if (new_len > old_len) {
        memset((char *)dream_p(p) + 4 + (size_t)old_len * (size_t)esize, 0,
               (size_t)(new_len - old_len) * (size_t)esize);
    }
    return p;
}

dream_ptr dream_to_bytes(dream_ptr value, int32_t size) {
    dream_ptr p = dream_array_new(size, 1);
    if (value && size > 0) {
        memcpy((char *)dream_p(p) + 4, dream_p(value), (size_t)size);
    }
    return p;
}

dream_ptr dream_from_bytes(dream_ptr bytes, int32_t size, int32_t tag) {
    dream_ptr p = dream_malloc(size, tag);
    memset(dream_p(p), 0, (size_t)size);
    if (bytes) {
        int32_t n = dream_i32(bytes)[0];
        if (n > size) {
            n = size;
        }
        memcpy(dream_p(p), (char *)dream_p(bytes) + 4, (size_t)n);
    }
    return p;
}

void string_copy_utf8(dream_ptr dst, int32_t dst_off, dream_ptr src, int32_t src_off, int32_t count) {
    if (count <= 0 || !dst || !src) {
        return;
    }
    memcpy((char *)dream_p(dst) + 4 + dst_off, (const char *)dream_str_units(src) + src_off,
           (size_t)count);
}

void array_store16(dream_ptr arr, int32_t off, int32_t u) {
    uint16_t v = (uint16_t)u;
    if (!arr) {
        return;
    }
    memcpy((char *)dream_p(arr) + 4 + off, &v, 2);
}

int32_t utf8_width_at(dream_ptr s, int32_t i) {
    (void)s;
    (void)i;
    return 2;
}

int32_t utf8_decode_at(dream_ptr s, int32_t i) {
    uint16_t u = 0;
    if (!s) {
        return 0;
    }
    memcpy(&u, (const char *)dream_str_units(s) + i, 2);
    return (int32_t)u;
}

int32_t dream_char_at(dream_ptr ptr, int32_t i) {
    return ptr ? (int32_t)dream_char_at_u(ptr, i) : 0;
}

int32_t dream_byte_at(dream_ptr ptr, int32_t i) {
    return ptr ? (int32_t)dream_byte_at_u(ptr, i) : 0;
}

void string_set(dream_ptr ptr, int32_t i, int32_t c) {
    uint16_t u = (uint16_t)c;
    if (!ptr) {
        return;
    }
    ((uint16_t *)dream_str_units(ptr))[i] = u;
}

dream_ptr string_substring_raw(dream_ptr ptr, int32_t start, int32_t end) {
    int32_t n;
    int32_t len;
    dream_ptr p;
    if (!ptr) {
        return dream_string_alloc(0);
    }
    n = dream_str_len(ptr);
    if (start < 0) {
        start = 0;
    }
    if (end < 0) {
        end = 0;
    }
    if (start > n) {
        start = n;
    }
    if (end > n) {
        end = n;
    }
    if (end < start) {
        end = start;
    }
    len = end - start;
    if (len <= 0) {
        return dream_string_alloc(0);
    }
    p = dream_string_alloc(len);
    memcpy(
        (char *)dream_p(p) + STRING_UNITS_OFFSET,
        dream_str_units(ptr) + start,
        (size_t)len << 1
    );
    return p;
}

dream_ptr string_clone(dream_ptr ptr) {
    int32_t n = dream_str_len(ptr);
    dream_ptr p;
    if (!ptr || n <= 0) {
        return dream_string_alloc(0);
    }
    p = dream_string_alloc(n);
    memcpy((char *)dream_p(p) + STRING_UNITS_OFFSET, dream_str_units(ptr), (size_t)n << 1);
    return p;
}

dream_ptr string_from_builder(dream_ptr bytes, int32_t len, int32_t scalars) {
    dream_ptr p;
    if (!bytes || len <= 0) {
        return dream_string_alloc(0);
    }
    if (scalars < 0) {
        scalars = len >> 1;
    }
    p = dream_malloc(len + 8, TAG_STRING);
    dream_i32(p)[0] = scalars;
    dream_str_init_owned(p);
    memcpy((char *)dream_p(p) + STRING_UNITS_OFFSET, (char *)dream_p(bytes) + 8, (size_t)len);
    return p;
}

static dream_ptr utf8_to_utf16(const uint8_t *src, int32_t n) {
    int32_t i = 0;
    int32_t o = 0;
    dream_ptr p;
    uint16_t *u;
    p = dream_string_alloc(n);
    u = (uint16_t *)((char *)dream_p(p) + STRING_UNITS_OFFSET);
    while (i < n) {
        uint32_t c = src[i++];
        if (c < 0x80) {
            u[o++] = (uint16_t)c;
        } else if ((c & 0xe0) == 0xc0 && i < n) {
            c = ((c & 0x1f) << 6) | (src[i++] & 0x3f);
            u[o++] = (uint16_t)c;
        } else if ((c & 0xf0) == 0xe0 && i + 1 < n) {
            c = ((c & 0x0f) << 12) | ((src[i] & 0x3f) << 6) | (src[i + 1] & 0x3f);
            i += 2;
            u[o++] = (uint16_t)c;
        } else {
            u[o++] = 0xfffd;
        }
    }
    dream_i32(p)[0] = o;
    return p;
}

dream_ptr string_from_utf8(dream_ptr bytes) {
    int32_t n;
    if (!bytes) {
        return dream_string_alloc(0);
    }
    n = dream_i32(bytes)[0];
    return utf8_to_utf16((const uint8_t *)((char *)dream_p(bytes) + 4), n);
}

dream_ptr string_from_utf8_prefix(dream_ptr bytes, int32_t len) {
    if (!bytes || len <= 0) {
        return dream_string_alloc(0);
    }
    return utf8_to_utf16((const uint8_t *)((char *)dream_p(bytes) + 4), len);
}

dream_ptr string_from_utf8_prefix_n(dream_ptr bytes, int32_t len, int32_t scalars) {
    dream_ptr p;
    int32_t count;
    if (!bytes) {
        return dream_string_alloc(0);
    }
    count = dream_i32(bytes)[0];
    if (len < 0) {
        len = 0;
    }
    if (len > count) {
        len = count;
    }
    if (len == 0) {
        return dream_string_alloc(0);
    }
    if (scalars < 0) {
        scalars = len >> 1;
    }
    p = dream_string_alloc(scalars);
    memcpy((char *)dream_p(p) + STRING_UNITS_OFFSET, (char *)dream_p(bytes) + 4, (size_t)len);
    return p;
}

int32_t string_compare(dream_ptr a, dream_ptr b) {
    int32_t na = dream_str_len(a);
    int32_t nb = dream_str_len(b);
    int32_t n = na < nb ? na : nb;
    int32_t i;
    const uint16_t *ua = dream_str_units(a);
    const uint16_t *ub = dream_str_units(b);
    for (i = 0; i < n; i++) {
        if (ua[i] != ub[i]) {
            return (int32_t)ua[i] - (int32_t)ub[i];
        }
    }
    return na - nb;
}
