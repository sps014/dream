#include "dream_rt.h"

IMPORT("malloc") int32_t rt_malloc(int32_t size, int32_t tag);
IMPORT("retain") void rt_retain(int32_t ptr);
IMPORT("release_generic") void rt_release(int32_t ptr);

EXPORT("string_fini")
void string_fini(int32_t p) {
    int32_t d;
    if (p == 0) {
        return;
    }
    if (i32_load(p - 8) != TAG_STRING) {
        return;
    }
    d = str_data(p);
    if (d == p + 8) {
        return;
    }
    rt_release(i32_load(p + 8));
}

EXPORT("string_release")
void string_release(int32_t p) {
    string_fini(p);
    rt_release(p);
}

EXPORT("str_byte_size")
int32_t str_byte_size(int32_t ptr) {
    return i32_load(ptr) << 1;
}

EXPORT("strlen")
int32_t strlen(int32_t ptr) {
    return str_byte_size(ptr);
}

EXPORT("utf8_width_at")
int32_t utf8_width_at(int32_t ptr, int32_t off) {
    (void)ptr;
    (void)off;
    return 2;
}

EXPORT("utf8_decode_at")
int32_t utf8_decode_at(int32_t ptr, int32_t off) {
    return (int32_t)u16_load(str_data(ptr) + off);
}

EXPORT("utf8_width_raw")
int32_t utf8_width_raw(int32_t base, int32_t off) {
    uint8_t b = u8_load(base + off);
    if (b < 0x80) {
        return 1;
    }
    if ((b & 0xE0) == 0xC0) {
        return 2;
    }
    if ((b & 0xF0) == 0xE0) {
        return 3;
    }
    return 4;
}

EXPORT("utf8_decode_raw")
int32_t utf8_decode_raw(int32_t base, int32_t off) {
    uint8_t b0 = u8_load(base + off);
    uint8_t b1;
    uint8_t b2;
    uint8_t b3;
    if (b0 < 0x80) {
        return (int32_t)b0;
    }
    if ((b0 & 0xE0) == 0xC0) {
        b1 = u8_load(base + off + 1);
        return ((b0 & 0x1F) << 6) | (b1 & 0x3F);
    }
    if ((b0 & 0xF0) == 0xE0) {
        b1 = u8_load(base + off + 1);
        b2 = u8_load(base + off + 2);
        return ((b0 & 0x0F) << 12) | ((b1 & 0x3F) << 6) | (b2 & 0x3F);
    }
    b1 = u8_load(base + off + 1);
    b2 = u8_load(base + off + 2);
    b3 = u8_load(base + off + 3);
    return ((b0 & 0x07) << 18) | ((b1 & 0x3F) << 12) | ((b2 & 0x3F) << 6) | (b3 & 0x3F);
}

EXPORT("utf16_encode_at")
int32_t utf16_encode_at(int32_t dst, int32_t i, int32_t cp) {
    if ((uint32_t)cp < 0x10000u) {
        u16_store(dst + (i << 1), (uint16_t)cp);
        return 1;
    }
    u16_store(dst + (i << 1), (uint16_t)(((uint32_t)cp - 0x10000u) >> 10) + 0xD800);
    u16_store(dst + ((i + 1) << 1), (uint16_t)(((uint32_t)cp - 0x10000u) & 0x3FF) + 0xDC00);
    return 2;
}

EXPORT("str_scalar_len")
int32_t str_scalar_len(int32_t ptr) {
    return i32_load(ptr);
}

static int32_t interned_empty(void) {
    int32_t empty = intern_empty();
    rt_retain(empty);
    return empty;
}

EXPORT("concat_strings")
int32_t concat_strings(int32_t str1, int32_t str2) {
    int32_t sc1 = i32_load(str1);
    int32_t len1 = sc1 << 1;
    int32_t sc2 = i32_load(str2);
    int32_t len2 = sc2 << 1;
    int32_t new_ptr;
    if (len1 == 0) {
        if (len2 == 0) {
            return interned_empty();
        }
        rt_retain(str2);
        return str2;
    }
    if (len2 == 0) {
        rt_retain(str1);
        return str1;
    }
    new_ptr = rt_malloc(len1 + len2 + 8, TAG_STRING);
    i32_store(new_ptr, sc1 + sc2);
    str_init_owned(new_ptr);
    mem_copy(new_ptr + 8, str_data(str1), len1);
    mem_copy(new_ptr + 8 + len1, str_data(str2), len2);
    return new_ptr;
}

EXPORT("concat_strings3")
int32_t concat_strings3(int32_t str1, int32_t str2, int32_t str3) {
    int32_t sc1 = i32_load(str1);
    int32_t len1 = sc1 << 1;
    int32_t sc2 = i32_load(str2);
    int32_t len2 = sc2 << 1;
    int32_t sc3 = i32_load(str3);
    int32_t len3 = sc3 << 1;
    int32_t new_ptr;
    int32_t off;
    if (len1 + len2 + len3 == 0) {
        return interned_empty();
    }
    if (len1 == 0) {
        return concat_strings(str2, str3);
    }
    if (len2 == 0) {
        return concat_strings(str1, str3);
    }
    if (len3 == 0) {
        return concat_strings(str1, str2);
    }
    new_ptr = rt_malloc(len1 + len2 + len3 + 8, TAG_STRING);
    i32_store(new_ptr, sc1 + sc2 + sc3);
    str_init_owned(new_ptr);
    mem_copy(new_ptr + 8, str_data(str1), len1);
    off = len1;
    mem_copy(new_ptr + 8 + off, str_data(str2), len2);
    off = off + len2;
    mem_copy(new_ptr + 8 + off, str_data(str3), len3);
    return new_ptr;
}

static int32_t int_ndigits(int32_t v) {
    int32_t neg = 0;
    int32_t tmp;
    int32_t ndigits;
    if (v == 0) {
        return 1;
    }
    tmp = v;
    if (v < 0) {
        neg = 1;
        tmp = -v;
    }
    if (tmp < 0) {
        ndigits = 10;
    } else if ((uint32_t)tmp < 10u) {
        ndigits = 1;
    } else if ((uint32_t)tmp < 100u) {
        ndigits = 2;
    } else if ((uint32_t)tmp < 1000u) {
        ndigits = 3;
    } else if ((uint32_t)tmp < 10000u) {
        ndigits = 4;
    } else if ((uint32_t)tmp < 100000u) {
        ndigits = 5;
    } else if ((uint32_t)tmp < 1000000u) {
        ndigits = 6;
    } else if ((uint32_t)tmp < 10000000u) {
        ndigits = 7;
    } else if ((uint32_t)tmp < 100000000u) {
        ndigits = 8;
    } else if ((uint32_t)tmp < 1000000000u) {
        ndigits = 9;
    } else {
        ndigits = 10;
    }
    if (neg) {
        ndigits = ndigits + 1;
    }
    return ndigits;
}

EXPORT("concat_str_int_str")
int32_t concat_str_int_str(int32_t pref, int32_t v, int32_t suf) {
    int32_t plen = i32_load(pref);
    int32_t slen = i32_load(suf);
    int32_t ndigits = int_ndigits(v);
    int32_t total = plen + ndigits + slen;
    int32_t p;
    int32_t d;
    int32_t pos;
    int32_t tmp;
    if (total == 0) {
        return interned_empty();
    }
    p = rt_malloc((total << 1) + 8, TAG_STRING);
    i32_store(p, total);
    str_init_owned(p);
    d = p + 8;
    if (plen != 0) {
        mem_copy(d, str_data(pref), plen << 1);
    }
    pos = d + (plen << 1);
    if (v == 0) {
        u16_store(pos, 48);
    } else {
        if (v < 0) {
            u16_store(pos, 45);
        }
        pos = d + ((plen + ndigits) << 1);
        tmp = v < 0 ? -v : v;
        while (tmp != 0) {
            pos = pos - 2;
            u16_store(pos, (uint16_t)((uint32_t)tmp % 10u + 48));
            tmp = (int32_t)((uint32_t)tmp / 10u);
        }
    }
    if (slen != 0) {
        mem_copy(d + ((plen + ndigits) << 1), str_data(suf), slen << 1);
    }
    return p;
}

EXPORT("debug_get_free_list_head")
int32_t debug_get_free_list_head(void) {
    return wasm_free_list_head();
}

EXPORT("debug_get_heap_ptr")
int32_t debug_get_heap_ptr(void) {
    return atomic_load_i32((int32_t)HEAP_PTR_ADDR);
}

EXPORT("debug_get_live_objects")
int32_t debug_get_live_objects(void) {
    return wasm_live_objects();
}

EXPORT("debug_get_total_allocations")
int32_t debug_get_total_allocations(void) {
    return wasm_total_allocations();
}

EXPORT("debug_get_ref_count")
int32_t debug_get_ref_count(int32_t ptr) {
    if (ptr == 0) {
        return 0;
    }
    return i32_load(ptr - 4);
}

EXPORT("string_eq")
int32_t string_eq(int32_t a, int32_t b) {
    int32_t len;
    int32_t words;
    int32_t i;
    if (a == b) {
        return 1;
    }
    if (a == 0 || b == 0) {
        return 0;
    }
    len = i32_load(a);
    if (len != i32_load(b)) {
        return 0;
    }
    len = len << 1;
    words = (uint32_t)len >> 2;
    i = 0;
    while ((uint32_t)i < (uint32_t)words) {
        if (i32_load(str_data(a) + (i << 2)) != i32_load(str_data(b) + (i << 2))) {
            return 0;
        }
        i = i + 1;
    }
    i = words << 2;
    while ((uint32_t)i < (uint32_t)len) {
        if (u8_load(str_data(a) + i) != u8_load(str_data(b) + i)) {
            return 0;
        }
        i = i + 1;
    }
    return 1;
}

EXPORT("string_compare")
int32_t string_compare(int32_t a, int32_t b) {
    int32_t len_a;
    int32_t len_b;
    int32_t n;
    int32_t words;
    int32_t i;
    int32_t wa;
    int32_t wb;
    int32_t ba;
    int32_t bb;
    if (a == b) {
        return 0;
    }
    len_a = a == 0 ? 0 : i32_load(a);
    len_b = b == 0 ? 0 : i32_load(b);
    len_a = len_a << 1;
    len_b = len_b << 1;
    n = (uint32_t)len_a < (uint32_t)len_b ? len_a : len_b;
    words = (uint32_t)n >> 2;
    i = 0;
    while ((uint32_t)i < (uint32_t)words) {
        wa = a == 0 ? 0 : i32_load(str_data(a) + (i << 2));
        wb = b == 0 ? 0 : i32_load(str_data(b) + (i << 2));
        if (wa != wb) {
            ba = 0;
            while (ba < 4) {
                bb = (wa >> (ba << 3)) & 255;
                if (bb != ((wb >> (ba << 3)) & 255)) {
                    if ((uint32_t)bb < (uint32_t)((wb >> (ba << 3)) & 255)) {
                        return -1;
                    }
                    return 1;
                }
                ba = ba + 1;
            }
        }
        i = i + 1;
    }
    i = words << 2;
    while ((uint32_t)i < (uint32_t)n) {
        ba = a == 0 ? 0 : (int32_t)u8_load(str_data(a) + i);
        bb = b == 0 ? 0 : (int32_t)u8_load(str_data(b) + i);
        if (ba != bb) {
            return (uint32_t)ba < (uint32_t)bb ? -1 : 1;
        }
        i = i + 1;
    }
    if ((uint32_t)len_a < (uint32_t)len_b) {
        return -1;
    }
    if ((uint32_t)len_a > (uint32_t)len_b) {
        return 1;
    }
    return 0;
}

EXPORT("string_substring_raw")
int32_t string_substring_raw(int32_t ptr, int32_t start, int32_t end) {
    int32_t sc;
    int32_t s;
    int32_t e;
    int32_t byte_start;
    int32_t scalars;
    int32_t byte_len;
    int32_t p;
    if (ptr == 0) {
        return interned_empty();
    }
    sc = i32_load(ptr);
    s = start;
    if (s < 0) {
        s = 0;
    }
    if ((uint32_t)s > (uint32_t)sc) {
        s = sc;
    }
    e = end;
    if (e < 0) {
        e = 0;
    }
    if ((uint32_t)e > (uint32_t)sc) {
        e = sc;
    }
    if ((uint32_t)e < (uint32_t)s) {
        e = s;
    }
    byte_start = s << 1;
    scalars = e - s;
    byte_len = scalars << 1;
    if (byte_len == 0) {
        return interned_empty();
    }
    p = rt_malloc(byte_len + 8, TAG_STRING);
    i32_store(p, scalars);
    str_init_owned(p);
    mem_copy(p + 8, str_data(ptr) + byte_start, byte_len);
    return p;
}

EXPORT("string_clone")
int32_t string_clone(int32_t ptr) {
    int32_t n;
    int32_t p;
    if (ptr == 0) {
        return interned_empty();
    }
    n = i32_load(ptr);
    if (n <= 0) {
        return interned_empty();
    }
    p = rt_malloc((n << 1) + 8, TAG_STRING);
    i32_store(p, n);
    str_init_owned(p);
    mem_copy(p + 8, str_data(ptr), n << 1);
    return p;
}

EXPORT("string_copy_utf8")
void string_copy_utf8(int32_t dst, int32_t dst_off, int32_t src, int32_t src_off, int32_t count) {
    if (count == 0 || dst == 0 || src == 0) {
        return;
    }
    mem_copy(dst + 4 + dst_off, str_data(src) + src_off, count);
}

EXPORT("char_at")
int32_t char_at(int32_t ptr, int32_t i) {
    return (int32_t)u16_load(str_data(ptr) + (i << 1));
}

EXPORT("byte_at")
int32_t byte_at(int32_t ptr, int32_t i) {
    return (int32_t)u8_load(str_data(ptr) + i);
}

EXPORT("string_alloc")
int32_t string_alloc(int32_t n) {
    int32_t p = rt_malloc((n << 1) + 8, TAG_STRING);
    i32_store(p, 0);
    str_init_owned(p);
    return p;
}

EXPORT("utf8_bytes_to_string")
int32_t utf8_bytes_to_string(int32_t base, int32_t byte_len) {
    int32_t p;
    int32_t off = 0;
    int32_t units = 0;
    int32_t dst;
    int32_t cp;
    int32_t w;
    if (byte_len == 0) {
        return interned_empty();
    }
    p = rt_malloc((byte_len << 1) + 8, TAG_STRING);
    dst = p + 8;
    while ((uint32_t)off < (uint32_t)byte_len) {
        cp = utf8_decode_raw(base, off);
        w = utf16_encode_at(dst, units, cp);
        units = units + w;
        off = off + utf8_width_raw(base, off);
    }
    i32_store(p, units);
    str_init_owned(p);
    return p;
}

EXPORT("string_from_utf8")
int32_t string_from_utf8(int32_t bytes) {
    if (bytes == 0) {
        return interned_empty();
    }
    return utf8_bytes_to_string(bytes + 4, i32_load(bytes));
}

EXPORT("string_set")
void string_set(int32_t ptr, int32_t i, int32_t c) {
    int32_t n = str_scalar_len(ptr);
    int32_t w;
    if ((uint32_t)i > (uint32_t)n) {
        __builtin_unreachable();
    }
    if ((uint32_t)c >= 0x10000u) {
        if (i == n) {
            w = utf16_encode_at(str_data(ptr), n, c);
            i32_store(ptr, n + w);
            return;
        }
        c = 0xFFFD;
    }
    u16_store(str_data(ptr) + (i << 1), (uint16_t)c);
    if (i == n) {
        i32_store(ptr, n + 1);
    }
}

EXPORT("string_from_utf8_prefix")
int32_t string_from_utf8_prefix(int32_t bytes, int32_t len) {
    int32_t count;
    if (bytes == 0) {
        return interned_empty();
    }
    count = i32_load(bytes);
    if (len < 0) {
        len = 0;
    }
    if (len > count) {
        len = count;
    }
    return utf8_bytes_to_string(bytes + 4, len);
}

EXPORT("string_from_utf8_prefix_n")
int32_t string_from_utf8_prefix_n(int32_t bytes, int32_t len, int32_t scalars) {
    int32_t count;
    int32_t p;
    if (bytes == 0) {
        return interned_empty();
    }
    count = i32_load(bytes);
    if (len < 0) {
        len = 0;
    }
    if (len > count) {
        len = count;
    }
    if (len == 0) {
        return interned_empty();
    }
    if (scalars < 0) {
        scalars = (int32_t)((uint32_t)len >> 1);
    }
    p = rt_malloc(len + 8, TAG_STRING);
    i32_store(p, scalars);
    str_init_owned(p);
    mem_copy(p + 8, bytes + 4, len);
    return p;
}

EXPORT("array_store16")
void array_store16(int32_t arr, int32_t off, int32_t u) {
    if (arr == 0) {
        return;
    }
    u16_store(arr + 4 + off, (uint16_t)u);
}

EXPORT("string_builder_append")
int32_t string_builder_append(int32_t bytes, int32_t count, int32_t text) {
    int32_t n;
    if (bytes == 0 || text == 0) {
        return count;
    }
    n = i32_load(text) << 1;
    if (n <= 0) {
        return count;
    }
    mem_copy(bytes + 8 + count, str_data(text), n);
    return count + n;
}

EXPORT("string_from_builder")
int32_t string_from_builder(int32_t bytes, int32_t len, int32_t scalars) {
    int32_t p;
    if (bytes == 0 || len <= 0) {
        return interned_empty();
    }
    if (scalars < 0) {
        scalars = (int32_t)((uint32_t)len >> 1);
    }
    p = rt_malloc(len + 8, TAG_STRING);
    i32_store(p, scalars);
    str_init_owned(p);
    mem_copy(p + 8, bytes + 8, len);
    return p;
}
