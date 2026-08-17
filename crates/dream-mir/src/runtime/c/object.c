#include "dream_rt.h"

IMPORT("malloc") int32_t rt_malloc(int32_t size, int32_t tag);
IMPORT("str_byte_size") int32_t rt_str_byte_size(int32_t ptr);
IMPORT("utf16_encode_at") int32_t rt_utf16_encode_at(int32_t dst, int32_t i, int32_t cp);

int32_t int_to_string(int32_t v);
int32_t ulong_to_string(int64_t v);

static int32_t box4(int32_t tag, int32_t v) {
    int32_t p = rt_malloc(4, tag);
    i32_store(p, v);
    return p;
}

EXPORT("box_int")
int32_t box_int(int32_t v) {
    return box4(TAG_INT, v);
}
EXPORT("box_float")
int32_t box_float(float v) {
    int32_t p = rt_malloc(4, TAG_FLOAT);
    f32_store(p, v);
    return p;
}
EXPORT("box_double")
int32_t box_double(double v) {
    int32_t p = rt_malloc(8, TAG_DOUBLE);
    f64_store(p, v);
    return p;
}
EXPORT("box_bool")
int32_t box_bool(int32_t v) {
    return box4(TAG_BOOL, v);
}
EXPORT("unbox_int")
int32_t unbox_int(int32_t p) {
    return i32_load(p);
}
EXPORT("unbox_float")
float unbox_float(int32_t p) {
    return f32_load(p);
}
EXPORT("unbox_double")
double unbox_double(int32_t p) {
    return f64_load(p);
}
EXPORT("unbox_bool")
int32_t unbox_bool(int32_t p) {
    return i32_load(p);
}
EXPORT("hash_int")
int32_t hash_int(int32_t v) {
    return v;
}
EXPORT("hash_bool")
int32_t hash_bool(int32_t v) {
    return v;
}
EXPORT("hash_float")
int32_t hash_float(float v) {
    return __builtin_bit_cast(int32_t, v);
}
EXPORT("hash_double")
int32_t hash_double(double v) {
    return __builtin_bit_cast(int32_t, (float)v);
}
EXPORT("hash_string")
int32_t hash_string(int32_t p) {
    int32_t h = -2128831035;
    int32_t len = rt_str_byte_size(p);
    int32_t i = 0;
    while ((uint32_t)i < (uint32_t)len) {
        h = (h ^ (int32_t)u8_load(p + 8 + i)) * 16777619;
        i = i + 1;
    }
    return h;
}

static void reverse_units(int32_t d, int32_t n) {
    int32_t start = 0;
    int32_t end = n - 1;
    while (start < end) {
        uint16_t tmp = u16_load(d + (start << 1));
        u16_store(d + (start << 1), u16_load(d + (end << 1)));
        u16_store(d + (end << 1), tmp);
        start = start + 1;
        end = end - 1;
    }
}

EXPORT("int_to_string")
int32_t int_to_string(int32_t v) {
    int32_t p = rt_malloc(32, TAG_STRING);
    int32_t d = p + 8;
    int32_t i = 0;
    int32_t neg = 0;
    int32_t digit;
    if (v == 0) {
        u16_store(d, 48);
        i32_store(p, 1);
        i32_store(p + 4, 0);
        return p;
    }
    if (v < 0) {
        neg = 1;
    }
    while (v != 0) {
        digit = v % 10;
        if (digit < 0) {
            digit = -digit;
        }
        u16_store(d + (i << 1), (uint16_t)(digit + 48));
        i = i + 1;
        v = v / 10;
    }
    if (neg) {
        u16_store(d + (i << 1), 45);
        i = i + 1;
    }
    i32_store(p, i);
    i32_store(p + 4, 0);
    reverse_units(d, i);
    return p;
}

EXPORT("box_char")
int32_t box_char(int32_t v) {
    return box4(TAG_CHAR, v);
}
EXPORT("unbox_char")
int32_t unbox_char(int32_t p) {
    return i32_load(p);
}
EXPORT("char_to_string")
int32_t char_to_string(int32_t v) {
    int32_t p = rt_malloc(16, TAG_STRING);
    int32_t w = rt_utf16_encode_at(p + 8, 0, v);
    i32_store(p, w);
    i32_store(p + 4, 0);
    return p;
}
EXPORT("box_byte")
int32_t box_byte(int32_t v) {
    return box4(TAG_BYTE, v);
}
EXPORT("box_uint")
int32_t box_uint(int32_t v) {
    return box4(TAG_UINT, v);
}
EXPORT("box_long")
int32_t box_long(int64_t v) {
    int32_t p = rt_malloc(8, TAG_LONG);
    i64_store(p, v);
    return p;
}
EXPORT("box_ulong")
int32_t box_ulong(int64_t v) {
    int32_t p = rt_malloc(8, TAG_ULONG);
    i64_store(p, v);
    return p;
}
EXPORT("unbox_byte")
int32_t unbox_byte(int32_t p) {
    return i32_load(p);
}
EXPORT("unbox_uint")
int32_t unbox_uint(int32_t p) {
    return i32_load(p);
}
EXPORT("unbox_long")
int64_t unbox_long(int32_t p) {
    return i64_load(p);
}
EXPORT("unbox_ulong")
int64_t unbox_ulong(int32_t p) {
    return i64_load(p);
}
EXPORT("byte_to_string")
int32_t byte_to_string(int32_t v) {
    return int_to_string(v);
}
EXPORT("uint_to_string")
int32_t uint_to_string(int32_t v) {
    return ulong_to_string((int64_t)(uint32_t)v);
}
EXPORT("hash_long")
int32_t hash_long(int64_t v) {
    return (int32_t)v ^ (int32_t)((uint64_t)v >> 32);
}

EXPORT("long_to_string")
int32_t long_to_string(int64_t v) {
    int32_t p = rt_malloc(56, TAG_STRING);
    int32_t d = p + 8;
    int32_t i = 0;
    int32_t neg = 0;
    int32_t digit;
    if (v == 0) {
        u16_store(d, 48);
        i32_store(p, 1);
        i32_store(p + 4, 0);
        return p;
    }
    if (v < 0) {
        neg = 1;
    }
    while (v != 0) {
        digit = (int32_t)(v % 10);
        if (digit < 0) {
            digit = -digit;
        }
        u16_store(d + (i << 1), (uint16_t)(digit + 48));
        i = i + 1;
        v = v / 10;
    }
    if (neg) {
        u16_store(d + (i << 1), 45);
        i = i + 1;
    }
    i32_store(p, i);
    i32_store(p + 4, 0);
    reverse_units(d, i);
    return p;
}

EXPORT("ulong_to_string")
int32_t ulong_to_string(int64_t v) {
    int32_t p = rt_malloc(56, TAG_STRING);
    int32_t d = p + 8;
    int32_t i = 0;
    uint64_t u = (uint64_t)v;
    int32_t digit;
    if (u == 0) {
        u16_store(d, 48);
        i32_store(p, 1);
        i32_store(p + 4, 0);
        return p;
    }
    while (u != 0) {
        digit = (int32_t)(u % 10);
        u16_store(d + (i << 1), (uint16_t)(digit + 48));
        i = i + 1;
        u = u / 10;
    }
    i32_store(p, i);
    i32_store(p + 4, 0);
    reverse_units(d, i);
    return p;
}
