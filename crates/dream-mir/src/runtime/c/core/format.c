#include "dream_core.h"

/* Debug live-object counter (defined in heap.c); immortals leave the count. */
static dream_ptr from_utf8(const char *s) {
    size_t n = strlen(s);
    size_t i;
    dream_ptr p = dream_string_alloc((int32_t)n);
    uint16_t *u = (uint16_t *)((char *)dream_p(p) + STRING_UNITS_OFFSET);
    for (i = 0; i < n; i++) {
        u[i] = (uint16_t)(unsigned char)s[i];
    }
    return p;
}

dream_ptr dream_int_to_string(int32_t v) {
    return dream_int_to_string_fast(v);
}

dream_ptr dream_uint_to_string(int32_t v) {
    uint32_t u = (uint32_t)v;
    int32_t n = u == 0 ? 1 : dream_u32_ndigits(u);
    dream_ptr p = dream_malloc((dream_size)n * 2 + 8, TAG_STRING);
    uint16_t *out;
    dream_i32(p)[0] = n;
    dream_str_init_owned(p);
    out = (uint16_t *)((char *)dream_p(p) + STRING_UNITS_OFFSET);
    if (u == 0) {
        out[0] = 48;
        return p;
    }
    while (u != 0) {
        out[--n] = (uint16_t)(48u + u % 10u);
        u /= 10u;
    }
    return p;
}

/* Decimal digits of `u`, written right-to-left starting before `end`; returns the start of the
 * digits. The caller must place a NUL at `end` (this function never writes it). */
static char *write_u64_digits(char *end, uint64_t u) {
    do {
        *--end = (char)('0' + (int)(u % 10u));
        u /= 10u;
    } while (u != 0);
    return end;
}

dream_ptr dream_long_to_string(int64_t v) {
    /* `-(uint64_t)v` is well-defined for INT64_MIN too (two's-complement wrap). */
    uint64_t u = v < 0 ? -(uint64_t)v : (uint64_t)v;
    char buf[24];
    buf[sizeof(buf) - 1] = 0;
    char *digits = write_u64_digits(buf + sizeof(buf) - 1, u);
    if (v < 0) {
        *--digits = '-';
    }
    return from_utf8(digits);
}

dream_ptr dream_ulong_to_string(int64_t v) {
    char buf[24];
    buf[sizeof(buf) - 1] = 0;
    char *digits = write_u64_digits(buf + sizeof(buf) - 1, (uint64_t)v);
    return from_utf8(digits);
}

dream_ptr dream_byte_to_string(int32_t v) {
    return dream_int_to_string(v & 255);
}

/* Interned `"true"` / `"false"` singletons, pinned immortal (rc == DREAM_RC_IMMORTAL) because
 * callers release them through ordinary ARC. */
static dream_ptr bool_intern[2];

static dream_ptr intern_bool(int32_t v) {
    dream_ptr *slot = &bool_intern[v ? 1 : 0];
    dream_ptr s = dream_singleton_get(slot);
    if (DREAM_UNLIKELY(!s)) {
        s = dream_singleton_publish(slot, from_utf8(v ? "true" : "false"));
    }
    return s;
}

dream_ptr dream_bool_to_string(int32_t v) {
    return intern_bool(v);
}

dream_ptr dream_char_to_string(int32_t v) {
    dream_ptr p = dream_string_alloc(1);
    ((uint16_t *)((char *)dream_p(p) + STRING_UNITS_OFFSET))[0] = (uint16_t)v;
    return p;
}
