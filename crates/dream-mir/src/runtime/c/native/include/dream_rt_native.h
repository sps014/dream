#ifndef DREAM_RT_NATIVE_H
#define DREAM_RT_NATIVE_H

#ifdef DREAM_WASM32
#include "../../include/dream_abi.h"
#else
#ifndef DREAM_NATIVE
#define DREAM_NATIVE 1
#endif
#include "../../include/dream_abi.h"
#endif
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#ifdef DREAM_WASM32
typedef int32_t dream_ptr;
#else
typedef uintptr_t dream_ptr;
#endif

/* 0 until `workerSpawn`; retain/malloc then skip atomics/mutexes. */
extern int dream_rt_mt;

#define DREAM_ALWAYS_INLINE static inline __attribute__((always_inline))

#ifdef DREAM_WASM32
dream_ptr dream_g0_get(void);
void dream_g0_set(dream_ptr v);
int32_t dream_instance_tid(void);
int32_t dream_next_tid(void);
#else
extern _Thread_local dream_ptr g0;
DREAM_ALWAYS_INLINE dream_ptr dream_g0_get(void) { return g0; }
DREAM_ALWAYS_INLINE void dream_g0_set(dream_ptr v) { g0 = v; }
#endif

DREAM_ALWAYS_INLINE void *dream_p(dream_ptr p) {
#ifdef DREAM_WASM32
    return (void *)(uintptr_t)(uint32_t)p;
#else
    return (void *)p;
#endif
}

DREAM_ALWAYS_INLINE int32_t *dream_i32(dream_ptr p) {
    return (int32_t *)dream_p(p);
}

#ifdef DREAM_WASM32
#define DREAM_BLOCK_HEADER HEAP_HEADER_SIZE
#else
#define DREAM_BLOCK_HEADER NATIVE_HEAP_HEADER_SIZE
#endif

#ifdef DREAM_WASM32
void abort(void);
#endif

#ifndef DREAM_STR_SLICE
#define DREAM_STR_SLICE 1
#endif

DREAM_ALWAYS_INLINE int32_t dream_str_len(dream_ptr str) {
    return str ? dream_i32(str)[0] : 0;
}

DREAM_ALWAYS_INLINE int32_t dream_str_byte_size(dream_ptr str) {
    return dream_str_len(str) << 1;
}

DREAM_ALWAYS_INLINE const uint16_t *dream_str_units(dream_ptr s) {
    const uint16_t *d;
    if (!s) {
        return NULL;
    }
    if (dream_i32(s)[1] == DREAM_STR_SLICE) {
        memcpy(&d, (char *)dream_p(s) + STRING_UNITS_OFFSET + sizeof(dream_ptr), sizeof(d));
        return d;
    }
    return (const uint16_t *)((char *)dream_p(s) + STRING_UNITS_OFFSET);
}

DREAM_ALWAYS_INLINE void dream_str_init_owned(dream_ptr p) {
    if (p) {
        dream_i32(p)[1] = DREAM_STR_PAD_INLINE;
    }
}

DREAM_ALWAYS_INLINE uint16_t dream_char_at_u(dream_ptr str, int32_t i) {
    const uint16_t *u = dream_str_units(str);
    return u ? u[i] : 0;
}

DREAM_ALWAYS_INLINE uint8_t dream_byte_at_u(dream_ptr str, int32_t i) {
    return ((const uint8_t *)dream_str_units(str))[i];
}

DREAM_ALWAYS_INLINE void dream_mem_copy(dream_ptr dst, dream_ptr src, size_t n) {
    if (n == 0 || dst == 0 || src == 0) {
        return;
    }
    memcpy(dream_p(dst), dream_p(src), n);
}

DREAM_ALWAYS_INLINE void dream_retain(dream_ptr ptr) {
    int32_t *rc;
    if (ptr == 0) {
        return;
    }
    rc = (int32_t *)((char *)dream_p(ptr) - RC_FROM_DATA);
    if (!dream_rt_mt) {
        if (*rc != INT32_MAX) {
            *rc += 1;
        }
        return;
    }
    if (__atomic_load_n(rc, __ATOMIC_RELAXED) == INT32_MAX) {
        return;
    }
    __atomic_fetch_add(rc, 1, __ATOMIC_RELAXED);
}

void dream_free(dream_ptr ptr);

extern _Thread_local int32_t dream_defer_depth;
extern _Thread_local int32_t dream_defer_busy;
void dream_defer_enter(void);
void dream_defer_leave(uint32_t q);
int dream_defer_try_enqueue(dream_ptr p, void (*fn)(dream_ptr));
void dream_defer_drain_all(void);

DREAM_ALWAYS_INLINE void dream_release(dream_ptr ptr) {
    int32_t *rc;
    int32_t old;
    if (ptr == 0) {
        return;
    }
    rc = (int32_t *)((char *)dream_p(ptr) - RC_FROM_DATA);
    if (!dream_rt_mt) {
        if (*rc == INT32_MAX) {
            return;
        }
        old = *rc;
        *rc = old - 1;
        if (old == 1) {
            dream_free(ptr);
        }
        return;
    }
    if (__atomic_load_n(rc, __ATOMIC_RELAXED) == INT32_MAX) {
        return;
    }
    old = __atomic_fetch_sub(rc, 1, __ATOMIC_ACQ_REL);
    if (old == 1) {
        dream_free(ptr);
    }
}

DREAM_ALWAYS_INLINE void dream_destroy(dream_ptr ptr) {
    int32_t *rc;
    if (ptr == 0) {
        return;
    }
    rc = (int32_t *)((char *)dream_p(ptr) - RC_FROM_DATA);
    if (*rc == INT32_MAX) {
        return;
    }
    dream_free(ptr);
}

dream_ptr dream_malloc(int32_t size, int32_t tag);
dream_ptr dream_realloc(dream_ptr ptr, int32_t new_size, int32_t tag);
#ifdef DREAM_WASM32
void dream_heap_init(void);
#endif

DREAM_ALWAYS_INLINE int32_t dream_str_rc(dream_ptr p) {
    if (p == 0) {
        return 0;
    }
    return ((int32_t *)((char *)dream_p(p) - RC_FROM_DATA))[0];
}

DREAM_ALWAYS_INLINE int dream_str_unique_owned(dream_ptr p) {
    if (p == 0 || dream_str_rc(p) != 1) {
        return 0;
    }
    if (((int32_t *)((char *)dream_p(p) - TAG_FROM_DATA))[0] != TAG_STRING) {
        return 0;
    }
    if (dream_i32(p)[1] == DREAM_STR_SLICE) {
        return 0;
    }
    return 1;
}

DREAM_ALWAYS_INLINE int32_t dream_str_unit_cap(dream_ptr p) {
    int32_t block;
    int32_t payload;
    if (p == 0) {
        return 0;
    }
    block = ((int32_t *)((char *)dream_p(p) - DREAM_BLOCK_HEADER))[0];
    payload = block - DREAM_BLOCK_HEADER;
    if (payload <= STRING_HEADER_SIZE) {
        return 0;
    }
    return (payload - STRING_HEADER_SIZE) / 2;
}

DREAM_ALWAYS_INLINE int dream_str_can_hold(dream_ptr p, int32_t units) {
    return dream_str_unique_owned(p) && units <= dream_str_unit_cap(p);
}

DREAM_ALWAYS_INLINE void dream_concat_fill(dream_ptr p, dream_ptr a, dream_ptr b, int32_t sc1,
                                           int32_t sc2) {
    size_t len1 = (size_t)sc1 << 1;
    size_t len2 = (size_t)sc2 << 1;
    dream_i32(p)[0] = sc1 + sc2;
    dream_str_init_owned(p);
    memcpy((char *)dream_p(p) + STRING_UNITS_OFFSET, dream_str_units(a), len1);
    memcpy((char *)dream_p(p) + STRING_UNITS_OFFSET + len1, dream_str_units(b), len2);
}

DREAM_ALWAYS_INLINE dream_ptr dream_concat_strings(dream_ptr a, dream_ptr b) {
    int32_t sc1 = dream_str_len(a);
    int32_t sc2 = dream_str_len(b);
    size_t len1 = (size_t)sc1 << 1;
    size_t len2 = (size_t)sc2 << 1;
    dream_ptr p;
    if (len1 == 0) {
        if (len2 == 0) {
            return 0;
        }
        dream_retain(b);
        return b;
    }
    if (len2 == 0) {
        dream_retain(a);
        return a;
    }
    /* Operand buffers stay immutable: RC==1 is not "dead". `"--" + boundary` would
     * otherwise rewrite a live `boundary`. Dest reuse is `_into` only. */
    p = dream_malloc((int32_t)(len1 + len2 + 8), TAG_STRING);
    dream_concat_fill(p, a, b, sc1, sc2);
    return p;
}

DREAM_ALWAYS_INLINE dream_ptr dream_concat_strings_into(dream_ptr dest, dream_ptr a, dream_ptr b) {
    int32_t sc1 = dream_str_len(a);
    int32_t sc2 = dream_str_len(b);
    dream_ptr p;
    if (sc1 == 0 && sc2 == 0) {
        dream_release(dest);
        return 0;
    }
    if (dest != a && dest != b && dream_str_can_hold(dest, sc1 + sc2)) {
        dream_concat_fill(dest, a, b, sc1, sc2);
        return dest;
    }
    p = dream_concat_strings(a, b);
    dream_release(dest);
    return p;
}

DREAM_ALWAYS_INLINE int32_t dream_u32_ndigits(uint32_t u) {
    if (u < 10u) {
        return 1;
    }
    if (u < 100u) {
        return 2;
    }
    if (u < 1000u) {
        return 3;
    }
    if (u < 10000u) {
        return 4;
    }
    if (u < 100000u) {
        return 5;
    }
    if (u < 1000000u) {
        return 6;
    }
    if (u < 10000000u) {
        return 7;
    }
    if (u < 100000000u) {
        return 8;
    }
    if (u < 1000000000u) {
        return 9;
    }
    return 10;
}

DREAM_ALWAYS_INLINE int32_t dream_i32_utf16_len(int32_t v) {
    if (v == 0) {
        return 1;
    }
    if (v < 0) {
        if (v == (int32_t)0x80000000) {
            return 11;
        }
        return 1 + dream_u32_ndigits((uint32_t)-v);
    }
    return dream_u32_ndigits((uint32_t)v);
}

DREAM_ALWAYS_INLINE void dream_write_i32_utf16_n(uint16_t *out, int32_t v, int32_t n) {
    uint32_t u;
    if (v == 0) {
        out[0] = 48;
        return;
    }
    if (v < 0) {
        out[0] = 45;
        if (v == (int32_t)0x80000000) {
            static const char min[] = "2147483648";
            int32_t i;
            for (i = 0; i < 10; i++) {
                out[1 + i] = (uint16_t)min[i];
            }
            return;
        }
        u = (uint32_t)-v;
    } else {
        u = (uint32_t)v;
    }
    while (u != 0) {
        out[--n] = (uint16_t)(48u + u % 10u);
        u /= 10u;
    }
}

DREAM_ALWAYS_INLINE void dream_write_i32_utf16(uint16_t *out, int32_t v) {
    dream_write_i32_utf16_n(out, v, dream_i32_utf16_len(v));
}

DREAM_ALWAYS_INLINE dream_ptr dream_int_to_string_fast(int32_t v) {
    int32_t n = dream_i32_utf16_len(v);
    dream_ptr     p = dream_malloc((int32_t)((size_t)n * 2 + 8), TAG_STRING);
    dream_i32(p)[0] = n;
    dream_str_init_owned(p);
    dream_write_i32_utf16((uint16_t *)((char *)dream_p(p) + STRING_UNITS_OFFSET), v);
    return p;
}

DREAM_ALWAYS_INLINE void dream_concat_str_int_str_fill(dream_ptr p, dream_ptr pref, int32_t v,
                                                       dream_ptr suf, int32_t plen, int32_t nd,
                                                       int32_t slen) {
    uint16_t *d = (uint16_t *)((char *)dream_p(p) + STRING_UNITS_OFFSET);
    dream_i32(p)[0] = plen + nd + slen;
    dream_str_init_owned(p);
    if (plen != 0) {
        memcpy(d, dream_str_units(pref), (size_t)plen << 1);
    }
    dream_write_i32_utf16_n(d + plen, v, nd);
    if (slen != 0) {
        memcpy(d + plen + nd, dream_str_units(suf), (size_t)slen << 1);
    }
}

DREAM_ALWAYS_INLINE dream_ptr dream_concat_str_int_str(dream_ptr pref, int32_t v, dream_ptr suf) {
    int32_t plen = dream_str_len(pref);
    int32_t slen = dream_str_len(suf);
    int32_t nd = dream_i32_utf16_len(v);
    int32_t total = plen + nd + slen;
    dream_ptr p;
    if (total <= 0) {
        return 0;
    }
    p = dream_malloc((int32_t)((size_t)total * 2 + 8), TAG_STRING);
    dream_concat_str_int_str_fill(p, pref, v, suf, plen, nd, slen);
    return p;
}

DREAM_ALWAYS_INLINE dream_ptr dream_concat_str_int_str_into(dream_ptr dest, dream_ptr pref,
                                                            int32_t v, dream_ptr suf) {
    int32_t plen = dream_str_len(pref);
    int32_t slen = dream_str_len(suf);
    int32_t nd = dream_i32_utf16_len(v);
    int32_t total = plen + nd + slen;
    dream_ptr p;
    if (total <= 0) {
        dream_release(dest);
        return 0;
    }
    if (dest != pref && dest != suf && dream_str_can_hold(dest, total)) {
        dream_concat_str_int_str_fill(dest, pref, v, suf, plen, nd, slen);
        return dest;
    }
    p = dream_concat_str_int_str(pref, v, suf);
    dream_release(dest);
    return p;
}

DREAM_ALWAYS_INLINE void dream_substring_clamp(dream_ptr s, int32_t *start, int32_t *end) {
    int32_t len = dream_str_len(s);
    if ((uint32_t)*start > (uint32_t)len) {
        *start = *start < 0 ? 0 : len;
    }
    if ((uint32_t)*end > (uint32_t)len) {
        *end = *end < 0 ? 0 : len;
    }
    if (*end < *start) {
        *end = *start;
    }
}

DREAM_ALWAYS_INLINE void dream_slice_retain_parent(dream_ptr s) {
    int32_t *rc = (int32_t *)((char *)dream_p(s) - RC_FROM_DATA);
    if (*rc == INT32_MAX) {
        return;
    }
    if (!dream_rt_mt) {
        *rc += 1;
    } else {
        dream_retain(s);
    }
}

DREAM_ALWAYS_INLINE void dream_slice_fill(dream_ptr p, dream_ptr s, int32_t n,
                                          const uint16_t *data) {
    dream_i32(p)[0] = n;
    dream_i32(p)[1] = DREAM_STR_SLICE;
    memcpy((char *)dream_p(p) + 8, &s, sizeof(s));
    memcpy((char *)dream_p(p) + 8 + sizeof(dream_ptr), &data, sizeof(data));
}

DREAM_ALWAYS_INLINE int dream_slice_unique(dream_ptr p) {
    if (p == 0 || dream_str_rc(p) != 1) {
        return 0;
    }
    if (((int32_t *)((char *)dream_p(p) - TAG_FROM_DATA))[0] != TAG_STRING) {
        return 0;
    }
    return dream_i32(p)[1] == DREAM_STR_SLICE;
}

dream_ptr dream_string_alloc(int32_t units);

DREAM_ALWAYS_INLINE dream_ptr dream_substring(dream_ptr s, int32_t start, int32_t end) {
    int32_t n;
    dream_ptr p;
    const uint16_t *data;
    if (s == 0) {
        return 0;
    }
    dream_substring_clamp(s, &start, &end);
    n = end - start;
    if (n <= 0) {
        return dream_string_alloc(0); /* immortal shared empty */
    }
    p = dream_malloc((int32_t)(8 + 2 * (int32_t)sizeof(dream_ptr)), TAG_STRING);
    data = dream_str_units(s) + start;
    dream_slice_fill(p, s, n, data);
    dream_slice_retain_parent(s);
    return p;
}

DREAM_ALWAYS_INLINE dream_ptr dream_substring_into(dream_ptr dest, dream_ptr s, int32_t start,
                                                   int32_t end) {
    int32_t n;
    const uint16_t *data;
    dream_ptr p;
    if (s == 0) {
        dream_release(dest);
        return 0;
    }
    dream_substring_clamp(s, &start, &end);
    n = end - start;
    if (dream_slice_unique(dest)) {
        dream_ptr old_parent = 0;
        memcpy(&old_parent, (char *)dream_p(dest) + 8, sizeof(old_parent));
        if (n <= 0) {
            dream_i32(dest)[0] = 0;
            dream_str_init_owned(dest);
            if (old_parent != 0) {
                dream_release(old_parent);
            }
            return dest;
        }
        data = dream_str_units(s) + start;
        if (old_parent != s) {
            dream_slice_retain_parent(s);
            if (old_parent != 0) {
                dream_release(old_parent);
            }
        }
        dream_slice_fill(dest, s, n, data);
        return dest;
    }
    p = dream_substring(s, start, end);
    dream_release(dest);
    return p;
}

#if defined(__AVX2__)
#define DREAM_F32_LANES 8
#define DREAM_I32_LANES 8
#elif defined(__ARM_NEON) || defined(__ARM_NEON__)
#define DREAM_F32_LANES 4
#define DREAM_I32_LANES 4
#else
#define DREAM_F32_LANES 4
#define DREAM_I32_LANES 4
#endif

typedef float dream_f32v __attribute__((vector_size(DREAM_F32_LANES * 4)));
typedef int32_t dream_i32v __attribute__((vector_size(DREAM_I32_LANES * 4)));
typedef float dream_f32x4 __attribute__((vector_size(16)));

DREAM_ALWAYS_INLINE void dream_arr_add_f32(float *c, const float *a, const float *b, int32_t n) {
    int32_t i = 0;
    for (; i + DREAM_F32_LANES <= n; i += DREAM_F32_LANES) {
        dream_f32v va;
        dream_f32v vb;
        memcpy(&va, a + i, sizeof(va));
        memcpy(&vb, b + i, sizeof(vb));
        va = va + vb;
        memcpy(c + i, &va, sizeof(va));
    }
    for (; i < n; i++) {
        c[i] = a[i] + b[i];
    }
}

DREAM_ALWAYS_INLINE void dream_arr_add_i32(int32_t *c, const int32_t *a, const int32_t *b, int32_t n) {
    int32_t i = 0;
    for (; i + DREAM_I32_LANES <= n; i += DREAM_I32_LANES) {
        dream_i32v va;
        dream_i32v vb;
        memcpy(&va, a + i, sizeof(va));
        memcpy(&vb, b + i, sizeof(vb));
        va = va + vb;
        memcpy(c + i, &va, sizeof(va));
    }
    for (; i < n; i++) {
        c[i] = a[i] + b[i];
    }
}

DREAM_ALWAYS_INLINE void dream_v128_splat_f32(void *dest, float v) {
    dream_f32x4 r = {v, v, v, v};
    memcpy(dest, &r, 16);
}

DREAM_ALWAYS_INLINE void dream_v128_f32_bin(void *dest, const void *a, const void *b, int32_t op) {
    dream_f32x4 x;
    dream_f32x4 y;
    memcpy(&x, a, 16);
    memcpy(&y, b, 16);
    if (op == 1) {
        x = x - y;
    } else if (op == 2) {
        x = x * y;
    } else if (op == 3 || op == 4) {
        float xa[4];
        float ya[4];
        float ra[4];
        int i;
        memcpy(xa, a, 16);
        memcpy(ya, b, 16);
        for (i = 0; i < 4; i++) {
            ra[i] = op == 3 ? (xa[i] < ya[i] ? xa[i] : ya[i]) : (xa[i] > ya[i] ? xa[i] : ya[i]);
        }
        memcpy(dest, ra, 16);
        return;
    } else {
        x = x + y;
    }
    memcpy(dest, &x, 16);
}

DREAM_ALWAYS_INLINE int32_t simd_lane_count(void) { return 4; }

DREAM_ALWAYS_INLINE dream_ptr simd_v128_load(dream_ptr arr, int32_t off) {
    return (dream_ptr)((char *)dream_p(arr) + 4 + (size_t)off * 4);
}

DREAM_ALWAYS_INLINE void simd_v128_store(dream_ptr v, dream_ptr dest, int32_t off) {
    memcpy((char *)dream_p(dest) + 4 + (size_t)off * 4, dream_p(v), 16);
}

DREAM_ALWAYS_INLINE dream_ptr simd_v128_splat(float v) {
    (void)v;
    return 0;
}

DREAM_ALWAYS_INLINE dream_ptr simd_v128_add(dream_ptr a, dream_ptr b) {
    (void)a;
    (void)b;
    return 0;
}

DREAM_ALWAYS_INLINE dream_ptr simd_v128_sub(dream_ptr a, dream_ptr b) {
    (void)a;
    (void)b;
    return 0;
}

DREAM_ALWAYS_INLINE dream_ptr simd_v128_mul(dream_ptr a, dream_ptr b) {
    (void)a;
    (void)b;
    return 0;
}

DREAM_ALWAYS_INLINE dream_ptr simd_v128_min(dream_ptr a, dream_ptr b) {
    (void)a;
    (void)b;
    return 0;
}

DREAM_ALWAYS_INLINE dream_ptr simd_v128_max(dream_ptr a, dream_ptr b) {
    (void)a;
    (void)b;
    return 0;
}

DREAM_ALWAYS_INLINE float simd_v128_sum(dream_ptr v) {
    const float *x = (const float *)dream_p(v);
    return x[0] + x[1] + x[2] + x[3];
}

DREAM_ALWAYS_INLINE void dream_simd_binop(dream_ptr dest, dream_ptr lhs, dream_ptr rhs, int32_t esize,
                                          int32_t op) {
    if (!dest || !lhs || !rhs) {
        return;
    }
    if (esize == 4 && op == 0) {
        dream_v128_f32_bin(dream_p(dest), dream_p(lhs), dream_p(rhs), 0);
        return;
    }
    if (esize == 8) {
        double a = *(const double *)dream_p(lhs);
        double b = *(const double *)dream_p(rhs);
        *(double *)dream_p(dest) = op == 1 ? a - b : op == 2 ? a * b : op == 3 ? a / b : a + b;
        return;
    }
    dream_v128_f32_bin(dream_p(dest), dream_p(lhs), dream_p(rhs), op);
}

DREAM_ALWAYS_INLINE int32_t dream_string_eq(dream_ptr a, dream_ptr b) {
    int32_t n;
    if (a == b) {
        return 1;
    }
    if (a == 0 || b == 0) {
        return 0;
    }
    n = dream_str_len(a);
    if (n != dream_str_len(b)) {
        return 0;
    }
    return memcmp(dream_str_units(a), dream_str_units(b), (size_t)n << 1) == 0;
}

DREAM_ALWAYS_INLINE int32_t dream_object_tag(dream_ptr p) {
    if (p == 0) {
        return 0;
    }
    return ((int32_t *)((char *)dream_p(p) - TAG_FROM_DATA))[0];
}

DREAM_ALWAYS_INLINE void dream_str_fini(dream_ptr p) {
    dream_ptr parent;
    if (!p || dream_object_tag(p) != TAG_STRING || dream_i32(p)[1] != DREAM_STR_SLICE) {
        return;
    }
    memcpy(&parent, (char *)dream_p(p) + 8, sizeof(parent));
    dream_i32(p)[1] = 0;
    dream_release(parent);
}

int32_t dream_hash_value(dream_ptr p);
dream_ptr dream_string_alloc(int32_t units);
dream_ptr dream_array_new(int32_t len, int32_t esize);
dream_ptr dream_array_realloc(dream_ptr arr, int32_t new_len, int32_t esize);

/* Native `StringBuilder` payload (bytes, count, capacity). `cap` mirrors the backing
 * `byte[]` length inline so the per-append fast path avoids a dependent load through the
 * array header; grow is the only writer. Typed so memcpy into the `byte[]` is not treated
 * as clobbering the builder fields. */
typedef struct {
    dream_ptr bytes;
    int32_t count;
    int32_t cap;
} dream_sb;

dream_ptr dream_sb_grow_bytes(dream_sb *sb, dream_ptr bytes, int32_t need);

DREAM_ALWAYS_INLINE const void *dream_str_units_fast(dream_ptr s) {
    const uint16_t *d;
    if (dream_i32(s)[1] == DREAM_STR_SLICE) {
        memcpy(&d, (char *)dream_p(s) + STRING_UNITS_OFFSET + sizeof(dream_ptr), sizeof(d));
        return d;
    }
    return (const char *)dream_p(s) + STRING_UNITS_OFFSET;
}

DREAM_ALWAYS_INLINE void dream_sb_push(dream_ptr sb, dream_ptr text) {
    dream_sb *restrict s;
    int32_t n;
    int32_t nbytes;
    int32_t count;
    int32_t cap;
    dream_ptr bytes;
    char *restrict dst;
    const char *restrict src;
    if (__builtin_expect(!sb || !text, 0)) {
        return;
    }
    n = dream_i32(text)[0];
    if (__builtin_expect(n <= 0, 0)) {
        return;
    }
    nbytes = n << 1;
    s = (dream_sb *)dream_p(sb);
    bytes = s->bytes;
    count = s->count;
    cap = s->cap;
    if (__builtin_expect(count + nbytes + 4 > cap, 0)) {
        bytes = dream_sb_grow_bytes(s, bytes, count + nbytes + 4);
        cap = s->cap;
    }
    src = (const char *)dream_str_units_fast(text);
    dst = (char *)dream_p(bytes) + STRING_UNITS_OFFSET + (size_t)(uint32_t)count;
    memcpy(dst, src, (size_t)nbytes);
    s->count = count + nbytes;
}

DREAM_ALWAYS_INLINE void dream_sb_push_units(dream_ptr sb, const void *src, int32_t n) {
    dream_sb *restrict s;
    int32_t nbytes;
    int32_t count;
    int32_t cap;
    dream_ptr bytes;
    char *restrict dst;
    if (__builtin_expect(!sb || !src || n <= 0, 0)) {
        return;
    }
    nbytes = n << 1;
    s = (dream_sb *)dream_p(sb);
    bytes = s->bytes;
    count = s->count;
    cap = s->cap;
    if (__builtin_expect(count + nbytes + 4 > cap, 0)) {
        bytes = dream_sb_grow_bytes(s, bytes, count + nbytes + 4);
        cap = s->cap;
    }
    dst = (char *)dream_p(bytes) + STRING_UNITS_OFFSET + (size_t)(uint32_t)count;
    memcpy(dst, src, (size_t)(uint32_t)nbytes);
    s->count = count + nbytes;
}

/* Digits are rendered into a stack buffer and copied once — replaces the old Dream-level
 * per-digit `ensure` + `store16` pairs in `StringBuilder.append_int/append_long`. */
DREAM_ALWAYS_INLINE void dream_sb_push_int(dream_ptr sb, int32_t v) {
    uint16_t buf[12];
    int32_t n = 12;
    uint32_t x;
    if (v == INT32_MIN) {
        dream_sb_push_units(sb, "-2147483648", 11);
        return;
    }
    x = (uint32_t)(v < 0 ? -v : v);
    do {
        buf[--n] = (uint16_t)('0' + (x % 10u));
        x /= 10u;
    } while (x);
    if (v < 0) {
        buf[--n] = (uint16_t)'-';
    }
    dream_sb_push_units(sb, buf + n, 12 - n);
}

DREAM_ALWAYS_INLINE void dream_sb_push_long(dream_ptr sb, int64_t v) {
    uint16_t buf[20];
    int32_t n = 20;
    uint64_t x;
    if (v == INT64_MIN) {
        dream_sb_push_units(sb, "-9223372036854775808", 20);
        return;
    }
    x = (uint64_t)(v < 0 ? -v : v);
    do {
        buf[--n] = (uint16_t)('0' + (int32_t)(x % 10ull));
        x /= 10ull;
    } while (x);
    if (v < 0) {
        buf[--n] = (uint16_t)'-';
    }
    dream_sb_push_units(sb, buf + n, 20 - n);
}
dream_ptr dream_array_to_string(dream_ptr arr);
dream_ptr dream_to_bytes(dream_ptr value, int32_t size);
dream_ptr dream_from_bytes(dream_ptr bytes, int32_t size, int32_t tag);
int32_t dream_string_hash(dream_ptr p);
int32_t dream_object_hash_code(dream_ptr p);
int32_t dream_bitcast_f32(float v);
int32_t dream_hash_double(double v);
int32_t dream_hash_long(int64_t v);
dream_ptr dream_object_to_string(dream_ptr p);
void dream_print_object(dream_ptr p);
void dream_panic(dream_ptr msg);

char *dream_string_to_utf8(dream_ptr s);
uint16_t *dream_string_to_utf16z(dream_ptr s);
dream_ptr dream_utf8_to_string(const char *s);
int64_t dream_ffi_read_ptr(int64_t base, int32_t index);
dream_ptr dream_ffi_read_cstring(int64_t ptr);

dream_ptr dream_int_to_string(int32_t v);
dream_ptr dream_uint_to_string(int32_t v);
dream_ptr dream_long_to_string(int64_t v);
dream_ptr dream_ulong_to_string(int64_t v);
dream_ptr dream_byte_to_string(int32_t v);
dream_ptr dream_bool_to_string(int32_t v);
dream_ptr dream_char_to_string(int32_t v);
dream_ptr dream_float_to_string(float v);
dream_ptr dream_double_to_string(double v);

dream_ptr dream_box_int(int32_t v);
dream_ptr dream_box_float(float v);
dream_ptr dream_box_double(double v);
dream_ptr dream_box_bool(int32_t v);
dream_ptr dream_box_char(int32_t v);
dream_ptr dream_box_long(int64_t v);
dream_ptr dream_box_uint(int32_t v);
dream_ptr dream_box_ulong(int64_t v);
dream_ptr dream_box_byte(int32_t v);
int32_t dream_unbox_int(dream_ptr p);
float dream_unbox_float(dream_ptr p);
double dream_unbox_double(dream_ptr p);
int32_t dream_unbox_bool(dream_ptr p);
int32_t dream_unbox_char(dream_ptr p);
int64_t dream_unbox_long(dream_ptr p);
int32_t dream_unbox_uint(dream_ptr p);
int64_t dream_unbox_ulong(dream_ptr p);
int32_t dream_unbox_byte(dream_ptr p);

dream_ptr dream_funcbox_new(int32_t idx, dream_ptr env);
int32_t dream_funcbox_funcidx(dream_ptr box);
dream_ptr dream_funcbox_env(dream_ptr box);
void dream_release_funcbox(dream_ptr box);

void dream_lock_acquire(dream_ptr lock_addr);
void dream_lock_release(dream_ptr lock_addr);
void dream_async_complete(dream_ptr future, dream_ptr value);
void dream_resolve(dream_ptr future, dream_ptr value);
void dream_cancel(dream_ptr future);
int32_t dream_async_await(dream_ptr future, dream_ptr *dest, int32_t resume_pc);
void dream_async_set_waker(dream_ptr future, dream_ptr self);
void dream_await(dream_ptr parent, dream_ptr child);
dream_ptr dream_new_future(int32_t size, int32_t poll, int32_t kind);
void dream_enqueue(dream_ptr f);
void dream_start(dream_ptr f);
void dream_run_loop(void);
dream_ptr dream_sleep(int32_t ms);
dream_ptr dream_all(dream_ptr arr, int32_t esize);
dream_ptr dream_any(dream_ptr arr);
dream_ptr delayMs(int32_t ms);
void *dream_ft_get(int32_t i);
int32_t utf8_width_at(dream_ptr s, int32_t i);
int32_t utf8_decode_at(dream_ptr s, int32_t i);
int32_t dream_lock_try_acquire(dream_ptr lock_addr);
int32_t dream_lock_try_acquire_for(dream_ptr lock_addr, int32_t timeout_ms);
void dream_semaphore_acquire(dream_ptr semaphore);
void dream_semaphore_release(dream_ptr semaphore);
int32_t dream_semaphore_try_acquire(dream_ptr semaphore);
int32_t dream_semaphore_try_acquire_for(dream_ptr semaphore, int32_t timeout_ms);
dream_ptr dream_js_call(dream_ptr target, dream_ptr via, dream_ptr method, int32_t argc);
void dream_weak_clear_all(dream_ptr obj);
extern int dream_weak_any;
void dream_weak_register(dream_ptr target, dream_ptr slot, int32_t kind, dream_ptr extra);
void dream_weak_unregister(dream_ptr target, dream_ptr slot);

int64_t regex_compile(dream_ptr pattern, int32_t flags);
void regex_free(int64_t h);
int32_t regex_group_count(int64_t h);
int32_t regex_name_count(int64_t h);
dream_ptr regex_name_at(int64_t h, int32_t i);
int32_t regex_name_number(int64_t h, int32_t i);
dream_ptr regex_find(int64_t h, dream_ptr input, int32_t pos);
int32_t regex_test(int64_t h, dream_ptr input);

int32_t debug_get_live_objects(void);
int32_t debug_get_total_allocations(void);
int32_t debug_get_ref_count(dream_ptr ptr);
int32_t debug_get_heap_ptr(void);
int32_t debug_get_free_list_head(void);

void string_copy_utf8(dream_ptr dst, int32_t dst_off, dream_ptr src, int32_t src_off, int32_t count);
void array_store16(dream_ptr arr, int32_t off, int32_t u);
void string_set(dream_ptr ptr, int32_t i, int32_t c);
dream_ptr string_from_builder(dream_ptr bytes, int32_t len, int32_t scalars);
dream_ptr string_substring_raw(dream_ptr ptr, int32_t start, int32_t end);
dream_ptr string_clone(dream_ptr ptr);
dream_ptr string_from_utf8(dream_ptr bytes);
dream_ptr string_from_utf8_prefix(dream_ptr bytes, int32_t len);
dream_ptr string_from_utf8_prefix_n(dream_ptr bytes, int32_t len, int32_t scalars);
int32_t string_compare(dream_ptr a, dream_ptr b);
int32_t dream_char_at(dream_ptr ptr, int32_t i);
int32_t dream_byte_at(dream_ptr ptr, int32_t i);

#ifdef DREAM_WASM32
#define DREAM_WASM_IMPORT(mod, name) __attribute__((import_module(mod), import_name(name)))
DREAM_WASM_IMPORT(DREAM_MODULE_ENV, DREAM_SYM_PRINT_INT) void print_int(int32_t v);
DREAM_WASM_IMPORT(DREAM_MODULE_ENV, DREAM_SYM_PRINT_STRING) void print_string(dream_ptr s);
DREAM_WASM_IMPORT(DREAM_MODULE_ENV, DREAM_SYM_PRINT_CHAR) void print_char(int32_t c);
DREAM_WASM_IMPORT(DREAM_MODULE_ENV, DREAM_SYM_PRINT_FLOAT) void print_float(float v);
DREAM_WASM_IMPORT(DREAM_MODULE_ENV, DREAM_SYM_PRINT_DOUBLE) void print_double(double v);
DREAM_WASM_IMPORT(DREAM_MODULE_HOST, DREAM_SYM_TIME_NOW_NANOS) int64_t timeNowNanos(void);
#else
int64_t timeNowNanos(void);
int64_t Time_nano_time(void);
int64_t processCpuTimeNanos(void);
int64_t processMemoryBytes(void);
int64_t dateNowMillis(void);
int32_t dateLocalOffsetMinutes(int64_t epoch_millis);
void print_int(int32_t v);
void print_string(dream_ptr s);
void print_char(int32_t c);
void print_float(float v);
void print_double(double v);

int32_t fileOpen(dream_ptr path, dream_ptr mode);
int32_t fileDelete(dream_ptr path);
int64_t fileSize(dream_ptr path);
dream_ptr fileReadBytes(dream_ptr path);
int64_t fileWriteBytes(dream_ptr path, dream_ptr data);
int32_t fileIsDir(dream_ptr path);
dream_ptr fileStat(dream_ptr path);
int32_t fileCopy(dream_ptr from, dream_ptr to);
int32_t fileRename(dream_ptr from, dream_ptr to);
dream_ptr dirList(dream_ptr path);
int32_t dirCreate(dream_ptr path);
int32_t dirCreateAll(dream_ptr path);
int32_t dirRemove(dream_ptr path);
int32_t dirRemoveAll(dream_ptr path);
dream_ptr fileHandleRead(int32_t fd, int32_t count);
int64_t fileHandleWrite(int32_t fd, dream_ptr data);
int32_t fileHandleSeek(int32_t fd, int64_t position);
int64_t fileHandleTell(int32_t fd);
int32_t fileHandleSeekEnd(int32_t fd, int64_t offset);
void fileHandleClose(int32_t fd);

void dream_process_capture_args(int32_t argc, char **argv);
int32_t processOsFamily(void);
dream_ptr processArgs(void);
dream_ptr processExePath(void);
void processEnvUnset(dream_ptr name);
dream_ptr processEnvKeys(void);
dream_ptr processTempDir(void);
dream_ptr processHomeDir(void);
dream_ptr consoleReadLine(void);
int32_t consoleReadKey(void);

void dream_host_bind(dream_ptr (*string_alloc)(int32_t), dream_ptr (*array_new)(int32_t, int32_t),
                     void (*complete_foreign)(dream_ptr, dream_ptr));
/* Complete an @async_host future from a foreign thread and wake the parked loop. */
void dream_complete_foreign(dream_ptr f, dream_ptr res);
/* Called by a delegate-shape poll thunk after handing work to a deferred host: keeps
 * dream_run_loop parked while the work is in flight. */
void dream_foreign_work_begin(void);
dream_ptr dream_worker_invoke(int32_t fn, dream_ptr env, dream_ptr arg);
int32_t workerSpawn(int32_t fn, int64_t env);
int32_t workerPoolSpawn(void);
void workerPost(int32_t id, dream_ptr msg);
dream_ptr workerRecv(int32_t id);
dream_ptr workerPoolDispatch(int32_t id, int32_t fn, int64_t env, dream_ptr msg);
void workerTerminate(int32_t id);

#endif /* !DREAM_WASM32 */

#endif
