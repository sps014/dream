#include "dream_rt.h"

#if defined(__wasm__)
#define DREAM_FREESTANDING 1
#endif

#ifndef DREAM_FREESTANDING
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#else
typedef unsigned long size_t;
void *memcpy(void *dst, const void *src, size_t n) { return __builtin_memcpy(dst, src, n); }
void *memmove(void *dst, const void *src, size_t n) { return __builtin_memmove(dst, src, n); }
void *memset(void *dst, int c, size_t n) { return __builtin_memset(dst, c, n); }
int memcmp(const void *a, const void *b, size_t n) { return __builtin_memcmp(a, b, n); }
static void abort(void) { __builtin_trap(); }
static int snprintf(char *buf, size_t n, const char *fmt, ...) {
    (void)buf;
    (void)n;
    (void)fmt;
    return 0;
}
#endif
#include <stdatomic.h>

static uint8_t *g_heap;
static uint32_t g_cap;
static uint32_t g_bump;
static atomic_int g_lock;
static int32_t g_live;
static int32_t g_total_alloc;

__attribute__((weak)) void dream_drop(int32_t ptr) { (void)ptr; }

static void grow(uint32_t need) {
#ifdef DREAM_FREESTANDING
    static uint8_t storage[16u * 1024u * 1024u];
    if (need > sizeof(storage)) {
        abort();
    }
    if (g_heap == 0) {
        g_heap = storage;
        g_cap = (uint32_t)sizeof(storage);
        memset(storage, 0, sizeof(storage));
    }
#else
    uint32_t n = g_cap == 0 ? (1u << 20) : g_cap;
    while (n < need) {
        n *= 2;
    }
    uint8_t *p = (uint8_t *)realloc(g_heap, n);
    if (!p) {
        fputs("dream-rt: out of memory\n", stderr);
        abort();
    }
    if (n > g_cap) {
        memset(p + g_cap, 0, n - g_cap);
    }
    g_heap = p;
    g_cap = n;
#endif
}

void dream_rt_init(void) {
    if (g_heap) {
        return;
    }
    grow(1u << 20);
    g_bump = DREAM_STRING_BASE;
}

uint8_t *dream_heap_base(void) {
    dream_rt_init();
    return g_heap;
}

static void lock_acq(void) {
    int expected = 0;
    while (!atomic_compare_exchange_weak(&g_lock, &expected, 1)) {
        expected = 0;
    }
}

static void lock_rel(void) { atomic_store(&g_lock, 0); }

int32_t dream_malloc(int32_t size, int32_t tag) {
    dream_rt_init();
    if (size < 0) {
        abort();
    }
    uint32_t payload = (uint32_t)size;
    uint32_t total = DREAM_HEAP_HEADER_SIZE + payload;
    if (total < 16) {
        total = 16;
    }
    lock_acq();
    uint32_t start = g_bump;
    uint32_t next = start + total;
    if (next > g_cap) {
        grow(next + (1u << 16));
    }
    g_bump = next;
    lock_rel();
    memset(g_heap + start, 0, total);
    memcpy(g_heap + start, &total, 4);
    memcpy(g_heap + start + DREAM_HEADER_TAG_OFFSET, &tag, 4);
    int32_t rc = 1;
    memcpy(g_heap + start + DREAM_HEADER_REFCOUNT_OFFSET, &rc, 4);
    if (tag != DREAM_TAG_STRING) {
        g_live += 1;
        g_total_alloc += 1;
    }
    return (int32_t)(start + DREAM_HEAP_HEADER_SIZE);
}

void dream_free(int32_t ptr) {
    (void)ptr;
}

static int32_t *rc_word(int32_t ptr) {
    if (g_heap == NULL || ptr < (int32_t)DREAM_HEAP_HEADER_SIZE || (uint32_t)ptr >= g_bump) {
        return NULL;
    }
    return (int32_t *)(g_heap + (uint32_t)ptr - DREAM_HEAP_HEADER_SIZE + DREAM_HEADER_REFCOUNT_OFFSET);
}

void dream_retain(int32_t ptr) {
    int32_t *rc = rc_word(ptr);
    if (!rc) {
        return;
    }
    *rc += 1;
}

void dream_release(int32_t ptr) {
    int32_t *rc = rc_word(ptr);
    if (!rc) {
        return;
    }
    *rc -= 1;
    if (*rc == 0) {
        *rc = -1;
        int32_t tag = dream_object_tag(ptr);
        if (tag != DREAM_TAG_STRING) {
            g_live -= 1;
        }
        static int depth;
        if (depth < 64) {
            depth += 1;
            dream_drop(ptr);
            depth -= 1;
        }
    }
}

void dream_retain_shared(int32_t ptr) {
    int32_t *rc = rc_word(ptr);
    if (!rc) {
        return;
    }
    atomic_fetch_add((_Atomic int32_t *)rc, 1);
}

void dream_release_shared(int32_t ptr) {
    int32_t *rc = rc_word(ptr);
    if (!rc) {
        return;
    }
    int32_t old = atomic_fetch_sub((_Atomic int32_t *)rc, 1);
    if (old == 1) {
        g_live -= 1;
        dream_drop(ptr);
    }
}

int32_t dream_load_i32(int32_t addr) {
    int32_t v;
    memcpy(&v, g_heap + (uint32_t)addr, 4);
    return v;
}

void dream_store_i32(int32_t addr, int32_t value) {
    memcpy(g_heap + (uint32_t)addr, &value, 4);
}

int64_t dream_load_i64(int32_t addr) {
    int64_t v;
    memcpy(&v, g_heap + (uint32_t)addr, 8);
    return v;
}

void dream_store_i64(int32_t addr, int64_t value) {
    memcpy(g_heap + (uint32_t)addr, &value, 8);
}

float dream_load_f32(int32_t addr) {
    float v;
    memcpy(&v, g_heap + (uint32_t)addr, 4);
    return v;
}

void dream_store_f32(int32_t addr, float value) {
    memcpy(g_heap + (uint32_t)addr, &value, 4);
}

double dream_load_f64(int32_t addr) {
    double v;
    memcpy(&v, g_heap + (uint32_t)addr, 8);
    return v;
}

void dream_store_f64(int32_t addr, double value) {
    memcpy(g_heap + (uint32_t)addr, &value, 8);
}

uint8_t dream_load_u8(int32_t addr) { return g_heap[(uint32_t)addr]; }

void dream_store_u8(int32_t addr, uint8_t value) { g_heap[(uint32_t)addr] = value; }

void dream_memzero(int32_t addr, int32_t n) {
    if (n > 0) {
        memset(g_heap + (uint32_t)addr, 0, (size_t)n);
    }
}

void dream_memcpy(int32_t dst, int32_t src, int32_t n) {
    if (n > 0) {
        memmove(g_heap + (uint32_t)dst, g_heap + (uint32_t)src, (size_t)n);
    }
}

int32_t dream_intern_utf8(const char *bytes, int32_t len) {
    if (len < 0) {
        abort();
    }
    int32_t ptr = dream_malloc(DREAM_STRING_HEADER_SIZE + len, DREAM_TAG_STRING);
    dream_store_i32(ptr, len);
    int32_t scalars = 0;
    int32_t i = 0;
    while (i < len) {
        unsigned char b = (unsigned char)bytes[i];
        if (b < 0x80) {
            i += 1;
        } else if ((b & 0xE0) == 0xC0) {
            i += 2;
        } else if ((b & 0xF0) == 0xE0) {
            i += 3;
        } else {
            i += 4;
        }
        scalars += 1;
    }
    dream_store_i32(ptr + 4, scalars);
    if (len > 0) {
        memcpy(g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET, bytes, (size_t)len);
    }
    return ptr;
}

int32_t dream_str_byte_size(int32_t ptr) {
    if (ptr <= 0) {
        return 0;
    }
    return dream_load_i32(ptr);
}

int32_t dream_str_scalar_len(int32_t ptr) {
    if (ptr <= 0) {
        return 0;
    }
    return dream_load_i32(ptr + 4);
}

int32_t dream_string_eq(int32_t a, int32_t b) {
    if (a == b) {
        return 1;
    }
    int32_t la = dream_str_byte_size(a);
    int32_t lb = dream_str_byte_size(b);
    if (la != lb) {
        return 0;
    }
    if (la <= 0) {
        return 1;
    }
    return memcmp(g_heap + (uint32_t)a + DREAM_STRING_UTF8_OFFSET,
                  g_heap + (uint32_t)b + DREAM_STRING_UTF8_OFFSET, (size_t)la) == 0;
}

int32_t dream_concat_strings(int32_t a, int32_t b) {
    int32_t la = dream_str_byte_size(a);
    int32_t lb = dream_str_byte_size(b);
    int32_t out = dream_malloc(DREAM_STRING_HEADER_SIZE + la + lb, DREAM_TAG_STRING);
    dream_store_i32(out, la + lb);
    dream_store_i32(out + 4, dream_str_scalar_len(a) + dream_str_scalar_len(b));
    if (la > 0) {
        memcpy(g_heap + (uint32_t)out + DREAM_STRING_UTF8_OFFSET,
               g_heap + (uint32_t)a + DREAM_STRING_UTF8_OFFSET, (size_t)la);
    }
    if (lb > 0) {
        memcpy(g_heap + (uint32_t)out + DREAM_STRING_UTF8_OFFSET + (uint32_t)la,
               g_heap + (uint32_t)b + DREAM_STRING_UTF8_OFFSET, (size_t)lb);
    }
    return out;
}

int32_t dream_char_at(int32_t ptr, int32_t index) {
    int32_t len = dream_str_byte_size(ptr);
    int32_t i = 0;
    int32_t n = 0;
    const unsigned char *p = g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET;
    while (i < len) {
        unsigned char b = p[i];
        int32_t w = 1;
        uint32_t cp = b;
        if (b < 0x80) {
            w = 1;
        } else if ((b & 0xE0) == 0xC0) {
            w = 2;
            cp = ((uint32_t)(b & 0x1F) << 6) | (p[i + 1] & 0x3F);
        } else if ((b & 0xF0) == 0xE0) {
            w = 3;
            cp = ((uint32_t)(b & 0x0F) << 12) | ((uint32_t)(p[i + 1] & 0x3F) << 6) | (p[i + 2] & 0x3F);
        } else {
            w = 4;
            cp = ((uint32_t)(b & 0x07) << 18) | ((uint32_t)(p[i + 1] & 0x3F) << 12) |
                 ((uint32_t)(p[i + 2] & 0x3F) << 6) | (p[i + 3] & 0x3F);
        }
        if (n == index) {
            return (int32_t)cp;
        }
        i += w;
        n += 1;
    }
    return 0;
}

int32_t dream_byte_at(int32_t ptr, int32_t index) {
    int32_t len = dream_str_byte_size(ptr);
    if (index < 0 || index >= len) {
        return 0;
    }
    return g_heap[(uint32_t)ptr + DREAM_STRING_UTF8_OFFSET + (uint32_t)index];
}

int32_t dream_array_len(int32_t ptr) {
    if (ptr <= 0) {
        return 0;
    }
    return dream_load_i32(ptr);
}

void dream_print_int(int32_t v) {
#ifndef DREAM_FREESTANDING
    printf("%d", v);
#else
    (void)v;
#endif
}
void dream_print_uint(int32_t v) {
#ifndef DREAM_FREESTANDING
    printf("%u", (unsigned)v);
#else
    (void)v;
#endif
}
void dream_print_long(int64_t v) {
#ifndef DREAM_FREESTANDING
    printf("%lld", (long long)v);
#else
    (void)v;
#endif
}
void dream_print_ulong(int64_t v) {
#ifndef DREAM_FREESTANDING
    printf("%llu", (unsigned long long)v);
#else
    (void)v;
#endif
}
void dream_print_float(float v) {
#ifndef DREAM_FREESTANDING
    printf("%.10g", (double)v);
#else
    (void)v;
#endif
}
void dream_print_double(double v) {
#ifndef DREAM_FREESTANDING
    printf("%.10g", v);
#else
    (void)v;
#endif
}
void dream_print_char(int32_t c) {
#ifndef DREAM_FREESTANDING
    if (c <= 0x7F) {
        putchar(c);
    } else {
        printf("%c", (char)c);
    }
#else
    (void)c;
#endif
}

void dream_print_string(int32_t ptr) {
#ifndef DREAM_FREESTANDING
    int32_t len = dream_str_byte_size(ptr);
    if (len > 0) {
        fwrite(g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET, 1, (size_t)len, stdout);
    }
#else
    (void)ptr;
#endif
}

void dream_print_newline(void) {
#ifndef DREAM_FREESTANDING
    putchar('\n');
#endif
}

void dream_panic(int32_t msg_ptr) {
#ifndef DREAM_FREESTANDING
    fputs("panic: ", stderr);
    int32_t len = dream_str_byte_size(msg_ptr);
    if (len > 0) {
        fwrite(g_heap + (uint32_t)msg_ptr + DREAM_STRING_UTF8_OFFSET, 1, (size_t)len, stderr);
    }
    fputc('\n', stderr);
#else
    (void)msg_ptr;
#endif
    abort();
}

void dream_weak_register(int32_t ptr) { (void)ptr; }
void dream_weak_clear_all(int32_t obj) { (void)obj; }

void dream_async_enqueue(int32_t future) { (void)future; }
int32_t dream_async_run(void) { return 0; }

int32_t dream_object_tag(int32_t ptr) {
    if (ptr <= 0) {
        return 0;
    }
    return dream_load_i32(ptr - (int32_t)(DREAM_HEAP_HEADER_SIZE - DREAM_HEADER_TAG_OFFSET));
}

int32_t dream_realloc(int32_t ptr, int32_t new_size, int32_t tag) {
    int32_t n = dream_malloc(new_size, tag);
    if (ptr <= 0) {
        return n;
    }
    int32_t old_total = dream_load_i32(ptr - (int32_t)DREAM_HEAP_HEADER_SIZE);
    int32_t old_payload = old_total - (int32_t)DREAM_HEAP_HEADER_SIZE;
    int32_t copy = old_payload < new_size ? old_payload : new_size;
    if (copy > 0) {
        dream_memcpy(n, ptr, copy);
    }
    return n;
}

int32_t dream_i32_to_string(int32_t v) {
    char buf[32];
    int n = snprintf(buf, sizeof(buf), "%d", v);
    return dream_intern_utf8(buf, n);
}

int32_t dream_i64_to_string(int64_t v) {
    char buf[32];
    int n = snprintf(buf, sizeof(buf), "%lld", (long long)v);
    return dream_intern_utf8(buf, n);
}

int32_t dream_hash_bytes(int32_t ptr) {
    if (ptr <= 0) {
        return 0;
    }
    int32_t len = dream_str_byte_size(ptr);
    uint32_t h = 2166136261u;
    const unsigned char *p = g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET;
    for (int32_t i = 0; i < len; i++) {
        h ^= p[i];
        h *= 16777619u;
    }
    return (int32_t)h;
}

void dream_lock_acquire(int32_t addr) {
    _Atomic int32_t *p = (_Atomic int32_t *)(g_heap + (uint32_t)addr);
    int expected = 0;
    while (!atomic_compare_exchange_weak(p, &expected, 1)) {
        expected = 0;
    }
}

void dream_lock_release(int32_t addr) {
    _Atomic int32_t *p = (_Atomic int32_t *)(g_heap + (uint32_t)addr);
    atomic_store(p, 0);
}

void dream_unimplemented(const char *name) {
#ifndef DREAM_FREESTANDING
    fprintf(stderr, "dream-rt: unimplemented host '%s'\n", name ? name : "?");
#else
    (void)name;
#endif
    abort();
}

static int32_t utf8_width(unsigned char b) {
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

static int32_t empty_string(void) {
    return dream_intern_utf8("", 0);
}

int32_t dream_string_alloc(int32_t n) {
    int32_t cap = n < 0 ? 0 : n;
    int32_t p = dream_malloc(8 + cap * 4, DREAM_TAG_STRING);
    dream_store_i32(p, 0);
    dream_store_i32(p + 4, 0);
    return p;
}

int32_t dream_string_from_utf8(int32_t bytes) {
    if (bytes <= 0) {
        return empty_string();
    }
    int32_t n = dream_load_i32(bytes);
    if (n <= 0) {
        return empty_string();
    }
    dream_rt_init();
    return dream_intern_utf8((const char *)(g_heap + (uint32_t)bytes + 4), n);
}

int32_t dream_string_from_utf8_prefix(int32_t bytes, int32_t len) {
    if (bytes <= 0) {
        return empty_string();
    }
    int32_t n = dream_load_i32(bytes);
    if (len < 0) {
        len = 0;
    }
    if (len > n) {
        len = n;
    }
    if (len <= 0) {
        return empty_string();
    }
    dream_rt_init();
    return dream_intern_utf8((const char *)(g_heap + (uint32_t)bytes + 4), len);
}

void dream_string_copy_utf8(int32_t dst, int32_t dst_off, int32_t src, int32_t src_off, int32_t count) {
    if (count <= 0 || dst <= 0 || src <= 0) {
        return;
    }
    dream_rt_init();
    memcpy(g_heap + (uint32_t)dst + 4 + (uint32_t)dst_off,
           g_heap + (uint32_t)src + DREAM_STRING_UTF8_OFFSET + (uint32_t)src_off,
           (size_t)count);
}

int32_t dream_string_clone(int32_t ptr) {
    int32_t n = dream_str_byte_size(ptr);
    if (n <= 0) {
        return empty_string();
    }
    dream_rt_init();
    return dream_intern_utf8((const char *)(g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET), n);
}

int32_t dream_string_compare(int32_t a, int32_t b) {
    int32_t la = dream_str_byte_size(a);
    int32_t lb = dream_str_byte_size(b);
    dream_rt_init();
    int32_t n = la < lb ? la : lb;
    int c = 0;
    if (n > 0) {
        c = memcmp(g_heap + (uint32_t)a + DREAM_STRING_UTF8_OFFSET,
                   g_heap + (uint32_t)b + DREAM_STRING_UTF8_OFFSET,
                   (size_t)n);
    }
    if (c != 0) {
        return c;
    }
    return la - lb;
}

int32_t dream_utf8_width_at(int32_t ptr, int32_t byte_off) {
    if (ptr <= 0) {
        return 1;
    }
    dream_rt_init();
    return utf8_width(g_heap[(uint32_t)ptr + DREAM_STRING_UTF8_OFFSET + (uint32_t)byte_off]);
}

int32_t dream_utf8_decode_at(int32_t ptr, int32_t byte_off) {
    if (ptr <= 0) {
        return 0;
    }
    dream_rt_init();
    const unsigned char *p = g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET + (uint32_t)byte_off;
    unsigned char b = p[0];
    if (b < 0x80) {
        return b;
    }
    if ((b & 0xE0) == 0xC0) {
        return ((b & 0x1F) << 6) | (p[1] & 0x3F);
    }
    if ((b & 0xF0) == 0xE0) {
        return ((b & 0x0F) << 12) | ((p[1] & 0x3F) << 6) | (p[2] & 0x3F);
    }
    return ((b & 0x07) << 18) | ((p[1] & 0x3F) << 12) | ((p[2] & 0x3F) << 6) | (p[3] & 0x3F);
}

static int32_t utf8_encode(uint8_t *p, int32_t cp) {
    if (cp < 0x80) {
        p[0] = (uint8_t)cp;
        return 1;
    }
    if (cp < 0x800) {
        p[0] = (uint8_t)(0xC0 | (cp >> 6));
        p[1] = (uint8_t)(0x80 | (cp & 0x3F));
        return 2;
    }
    if (cp < 0x10000) {
        p[0] = (uint8_t)(0xE0 | (cp >> 12));
        p[1] = (uint8_t)(0x80 | ((cp >> 6) & 0x3F));
        p[2] = (uint8_t)(0x80 | (cp & 0x3F));
        return 3;
    }
    p[0] = (uint8_t)(0xF0 | (cp >> 18));
    p[1] = (uint8_t)(0x80 | ((cp >> 12) & 0x3F));
    p[2] = (uint8_t)(0x80 | ((cp >> 6) & 0x3F));
    p[3] = (uint8_t)(0x80 | (cp & 0x3F));
    return 4;
}

static int32_t scalar_byte_off(int32_t ptr, int32_t idx) {
    int32_t n = dream_str_byte_size(ptr);
    dream_rt_init();
    const uint8_t *p = g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET;
    int32_t i = 0;
    int32_t s = 0;
    while (s < idx && i < n) {
        i += utf8_width(p[i]);
        s += 1;
    }
    return i;
}

int32_t dream_string_substring_raw(int32_t ptr, int32_t start, int32_t end) {
    if (ptr <= 0 || end <= start) {
        return empty_string();
    }
    int32_t b0 = scalar_byte_off(ptr, start);
    int32_t b1 = scalar_byte_off(ptr, end);
    int32_t n = b1 - b0;
    if (n <= 0) {
        return empty_string();
    }
    dream_rt_init();
    return dream_intern_utf8((const char *)(g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET + (uint32_t)b0), n);
}

void dream_string_set(int32_t ptr, int32_t i, int32_t c) {
    if (ptr <= 0) {
        return;
    }
    int32_t slen = dream_str_scalar_len(ptr);
    int32_t blen = dream_str_byte_size(ptr);
    dream_rt_init();
    uint8_t tmp[4];
    int32_t nw = utf8_encode(tmp, c);
    if (i == slen) {
        memcpy(g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET + (uint32_t)blen, tmp, (size_t)nw);
        dream_store_i32(ptr, blen + nw);
        dream_store_i32(ptr + 4, slen + 1);
        return;
    }
    int32_t b0 = scalar_byte_off(ptr, i);
    int32_t ow = utf8_width(g_heap[(uint32_t)ptr + DREAM_STRING_UTF8_OFFSET + (uint32_t)b0]);
    if (ow == nw) {
        memcpy(g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET + (uint32_t)b0, tmp, (size_t)nw);
        return;
    }
    int32_t rest = blen - b0 - ow;
    if (rest > 0) {
        memmove(g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET + (uint32_t)b0 + (uint32_t)nw,
                g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET + (uint32_t)b0 + (uint32_t)ow,
                (size_t)rest);
    }
    memcpy(g_heap + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET + (uint32_t)b0, tmp, (size_t)nw);
    dream_store_i32(ptr, blen - ow + nw);
}

static uint8_t *arr4(int32_t arr, int32_t off) {
    dream_rt_init();
    return g_heap + (uint32_t)arr + 4 + (uint32_t)off * 4;
}

void dream_simd_f32x4_add(int32_t dest, int32_t doff, int32_t a, int32_t aoff, int32_t b, int32_t boff) {
    float *d = (float *)arr4(dest, doff);
    float *x = (float *)arr4(a, aoff);
    float *y = (float *)arr4(b, boff);
    for (int i = 0; i < 4; i++) {
        d[i] = x[i] + y[i];
    }
}
void dream_simd_f32x4_sub(int32_t dest, int32_t doff, int32_t a, int32_t aoff, int32_t b, int32_t boff) {
    float *d = (float *)arr4(dest, doff);
    float *x = (float *)arr4(a, aoff);
    float *y = (float *)arr4(b, boff);
    for (int i = 0; i < 4; i++) {
        d[i] = x[i] - y[i];
    }
}
void dream_simd_f32x4_mul(int32_t dest, int32_t doff, int32_t a, int32_t aoff, int32_t b, int32_t boff) {
    float *d = (float *)arr4(dest, doff);
    float *x = (float *)arr4(a, aoff);
    float *y = (float *)arr4(b, boff);
    for (int i = 0; i < 4; i++) {
        d[i] = x[i] * y[i];
    }
}
void dream_simd_i32x4_add(int32_t dest, int32_t doff, int32_t a, int32_t aoff, int32_t b, int32_t boff) {
    int32_t *d = (int32_t *)arr4(dest, doff);
    int32_t *x = (int32_t *)arr4(a, aoff);
    int32_t *y = (int32_t *)arr4(b, boff);
    for (int i = 0; i < 4; i++) {
        d[i] = x[i] + y[i];
    }
}

int64_t dream_nano_time(void) {
#ifndef DREAM_FREESTANDING
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000LL + (int64_t)ts.tv_nsec;
#else
    return 0;
#endif
}

int64_t dream_now_millis(void) {
    return dream_nano_time() / 1000000LL;
}

int32_t dream_box_i32(int32_t v, int32_t tag) {
    int32_t p = dream_malloc(4, tag);
    dream_store_i32(p, v);
    return p;
}

int32_t dream_unbox_i32(int32_t ptr) {
    if (ptr <= 0) {
        return 0;
    }
    return dream_load_i32(ptr);
}

int32_t dream_box_i64(int64_t v, int32_t tag) {
    int32_t p = dream_malloc(8, tag);
    dream_store_i64(p, v);
    return p;
}

int64_t dream_unbox_i64(int32_t ptr) {
    if (ptr <= 0) {
        return 0;
    }
    return dream_load_i64(ptr);
}

int32_t dream_box_f32(float v) {
    int32_t p = dream_malloc(4, DREAM_TAG_FLOAT);
    dream_store_f32(p, v);
    return p;
}

float dream_unbox_f32(int32_t ptr) {
    if (ptr <= 0) {
        return 0.0f;
    }
    return dream_load_f32(ptr);
}

int32_t dream_box_f64(double v) {
    int32_t p = dream_malloc(8, DREAM_TAG_DOUBLE);
    dream_store_f64(p, v);
    return p;
}

double dream_unbox_f64(int32_t ptr) {
    if (ptr <= 0) {
        return 0.0;
    }
    return dream_load_f64(ptr);
}

int32_t dream_f32_to_string(float v) {
    char buf[64];
    int n = snprintf(buf, sizeof(buf), "%g", (double)v);
    return dream_intern_utf8(buf, n);
}

int32_t dream_f64_to_string(double v) {
    char buf[64];
    int n = snprintf(buf, sizeof(buf), "%g", v);
    return dream_intern_utf8(buf, n);
}

int32_t dream_bool_to_string(int32_t v) {
    const char *s = v ? "true" : "false";
    return dream_intern_utf8(s, v ? 4 : 5);
}

void dream_print_bool(int32_t v) {
#ifndef DREAM_FREESTANDING
    fputs(v ? "true" : "false", stdout);
#else
    (void)v;
#endif
}

int32_t debug_get_live_objects(void) { return g_live; }
int32_t debug_get_total_allocations(void) { return g_total_alloc; }
int32_t debug_get_heap_ptr(void) {
    dream_rt_init();
    return (int32_t)g_bump;
}
int32_t debug_get_free_list_head(void) { return 0; }
int32_t debug_get_ref_count(int32_t ptr) {
    int32_t *rc = rc_word(ptr);
    return rc ? *rc : 0;
}
