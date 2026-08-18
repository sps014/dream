#include "include/dream_rt_native.h"

#include <inttypes.h>
#include <stdio.h>

static dream_ptr from_utf8(const char *s) {
    size_t n = strlen(s);
    size_t i;
    dream_ptr p = dream_string_alloc((int32_t)n);
    uint16_t *u = (uint16_t *)((char *)dream_p(p) + STRING_UTF8_OFFSET);
    for (i = 0; i < n; i++) {
        u[i] = (uint16_t)(unsigned char)s[i];
    }
    return p;
}

dream_ptr dream_int_to_string(int32_t v) {
    char buf[32];
    snprintf(buf, sizeof(buf), "%d", v);
    return from_utf8(buf);
}

dream_ptr dream_uint_to_string(int32_t v) {
    char buf[32];
    snprintf(buf, sizeof(buf), "%u", (unsigned)v);
    return from_utf8(buf);
}

dream_ptr dream_long_to_string(int64_t v) {
    char buf[32];
    snprintf(buf, sizeof(buf), "%" PRId64, v);
    return from_utf8(buf);
}

dream_ptr dream_ulong_to_string(int64_t v) {
    char buf[32];
    snprintf(buf, sizeof(buf), "%" PRIu64, (uint64_t)v);
    return from_utf8(buf);
}

dream_ptr dream_byte_to_string(int32_t v) {
    return dream_int_to_string(v & 255);
}

dream_ptr dream_bool_to_string(int32_t v) {
    return from_utf8(v ? "true" : "false");
}

dream_ptr dream_char_to_string(int32_t v) {
    char buf[8];
    if (v < 128) {
        buf[0] = (char)v;
        buf[1] = 0;
        return from_utf8(buf);
    }
    snprintf(buf, sizeof(buf), "?");
    return from_utf8(buf);
}

dream_ptr dream_float_to_string(float v) {
    char buf[64];
    snprintf(buf, sizeof(buf), "%g", (double)v);
    return from_utf8(buf);
}

dream_ptr dream_double_to_string(double v) {
    char buf[64];
    snprintf(buf, sizeof(buf), "%g", v);
    return from_utf8(buf);
}
