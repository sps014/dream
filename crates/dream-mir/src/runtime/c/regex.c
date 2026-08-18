/* WASM PCRE2 wrapper. Compiled with wasi-sdk + vendored PCRE2 (interpreter, no JIT)
 * and spliced as runtime/regex.wat. Pointers are wasm32 linear-memory addresses. */
#define PCRE2_CODE_UNIT_WIDTH 16
#define PCRE2_STATIC 1
#define PCRE2_WASM 1
#include "pcre2.h"
#include "include/dream_abi.h"

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define RF_IGNORE_CASE 2
#define RF_MULTILINE 4
#define RF_DOTALL 8

typedef struct {
    pcre2_code *code;
    uint32_t capture_count;
} DreamRe;

extern int32_t malloc_tagged(int32_t size, int32_t tag);
extern void free_tagged(int32_t ptr);
extern int32_t __rt_str_empty_get(void);

static uint16_t *str_units(int32_t s) {
    int32_t d;
    if (s == 0) {
        return NULL;
    }
    d = ((int32_t *)(uintptr_t)(uint32_t)s)[1];
    if (d == 0) {
        return (uint16_t *)(uintptr_t)(uint32_t)(s + 8);
    }
    return (uint16_t *)(uintptr_t)(uint32_t)d;
}

static int32_t str_len(int32_t s) {
    return s ? ((int32_t *)(uintptr_t)(uint32_t)s)[0] : 0;
}

static int32_t string_from_units(const PCRE2_UCHAR *u, int32_t n) {
    int32_t p;
    if (n <= 0) {
        return __rt_str_empty_get();
    }
    p = malloc_tagged(n * 2 + 8, TAG_STRING);
    ((int32_t *)(uintptr_t)(uint32_t)p)[0] = n;
    ((int32_t *)(uintptr_t)(uint32_t)p)[1] = p + 8;
    memcpy((void *)(uintptr_t)(uint32_t)(p + 8), u, (size_t)n * 2);
    return p;
}

static int32_t array_i32(int32_t count) {
    int32_t p = malloc_tagged(4 + count * 4, TAG_ARRAY);
    int32_t i;
    ((int32_t *)(uintptr_t)(uint32_t)p)[0] = count;
    for (i = 0; i < count; i++) {
        ((int32_t *)(uintptr_t)(uint32_t)(p + 4))[i] = 0;
    }
    return p;
}

static uint32_t compile_opts(int32_t flags) {
    uint32_t opt = PCRE2_UTF;
    if (flags & RF_IGNORE_CASE) {
        opt |= PCRE2_CASELESS;
    }
    if (flags & RF_MULTILINE) {
        opt |= PCRE2_MULTILINE;
    }
    if (flags & RF_DOTALL) {
        opt |= PCRE2_DOTALL;
    }
    return opt;
}

static DreamRe *as_re(int64_t h) {
    if (h == 0) {
        return NULL;
    }
    return (DreamRe *)(uintptr_t)(uint64_t)h;
}

int64_t regex_compile(int32_t pattern, int32_t flags) {
    DreamRe *re;
    pcre2_code *code;
    int err = 0;
    PCRE2_SIZE err_off = 0;
    uint32_t ncap = 0;
    int32_t n;
    const PCRE2_UCHAR *pat;
    if (!pattern) {
        return 0;
    }
    n = str_len(pattern);
    pat = (const PCRE2_UCHAR *)str_units(pattern);
    code = pcre2_compile(pat, (PCRE2_SIZE)n, compile_opts(flags), &err, &err_off, NULL);
    if (code == NULL) {
        return 0;
    }
    (void)pcre2_pattern_info(code, PCRE2_INFO_CAPTURECOUNT, &ncap);
    re = (DreamRe *)malloc(sizeof(DreamRe));
    if (re == NULL) {
        pcre2_code_free(code);
        return 0;
    }
    re->code = code;
    re->capture_count = ncap;
    return (int64_t)(uintptr_t)re;
}

void regex_free(int64_t h) {
    DreamRe *re = as_re(h);
    if (re == NULL) {
        return;
    }
    pcre2_code_free(re->code);
    free(re);
}

int32_t regex_group_count(int64_t h) {
    DreamRe *re = as_re(h);
    return re ? (int32_t)re->capture_count : 0;
}

int32_t regex_name_count(int64_t h) {
    DreamRe *re = as_re(h);
    uint32_t n = 0;
    if (re == NULL) {
        return 0;
    }
    (void)pcre2_pattern_info(re->code, PCRE2_INFO_NAMECOUNT, &n);
    return (int32_t)n;
}

static const PCRE2_UCHAR *name_entry(DreamRe *re, int32_t i, uint32_t *entry_size) {
    PCRE2_SPTR table = NULL;
    uint32_t n = 0;
    uint32_t es = 0;
    (void)pcre2_pattern_info(re->code, PCRE2_INFO_NAMETABLE, &table);
    (void)pcre2_pattern_info(re->code, PCRE2_INFO_NAMECOUNT, &n);
    (void)pcre2_pattern_info(re->code, PCRE2_INFO_NAMEENTRYSIZE, &es);
    *entry_size = es;
    if (table == NULL || i < 0 || (uint32_t)i >= n || es == 0) {
        return NULL;
    }
    return table + (PCRE2_SIZE)i * es;
}

int32_t regex_name_at(int64_t h, int32_t i) {
    DreamRe *re = as_re(h);
    uint32_t es = 0;
    const PCRE2_UCHAR *ent;
    int32_t n = 0;
    if (re == NULL) {
        return __rt_str_empty_get();
    }
    ent = name_entry(re, i, &es);
    if (ent == NULL || es < 2) {
        return __rt_str_empty_get();
    }
    while (n + 1 < (int32_t)es && ent[1 + n] != 0) {
        n++;
    }
    return string_from_units(ent + 1, n);
}

int32_t regex_name_number(int64_t h, int32_t i) {
    DreamRe *re = as_re(h);
    uint32_t es = 0;
    const PCRE2_UCHAR *ent;
    if (re == NULL) {
        return 0;
    }
    ent = name_entry(re, i, &es);
    if (ent == NULL) {
        return 0;
    }
    return (int32_t)ent[0];
}

int32_t regex_find(int64_t h, int32_t input, int32_t pos) {
    DreamRe *re = as_re(h);
    pcre2_match_data *md;
    int rc;
    int32_t n;
    int32_t pairs;
    int32_t i;
    int32_t out;
    int32_t *dst;
    PCRE2_SIZE *ov;
    const PCRE2_UCHAR *sub;
    if (re == NULL || !input) {
        return array_i32(0);
    }
    n = str_len(input);
    if (pos < 0) {
        pos = 0;
    }
    if (pos > n) {
        return array_i32(0);
    }
    md = pcre2_match_data_create_from_pattern(re->code, NULL);
    if (md == NULL) {
        return array_i32(0);
    }
    sub = (const PCRE2_UCHAR *)str_units(input);
    rc = pcre2_match(re->code, sub, (PCRE2_SIZE)n, (PCRE2_SIZE)pos, 0, md, NULL);
    if (rc < 0) {
        pcre2_match_data_free(md);
        return array_i32(0);
    }
    pairs = (int32_t)pcre2_get_ovector_count(md);
    ov = pcre2_get_ovector_pointer(md);
    out = array_i32(pairs * 2);
    dst = (int32_t *)(uintptr_t)(uint32_t)(out + 4);
    for (i = 0; i < pairs; i++) {
        PCRE2_SIZE a = ov[2 * i];
        PCRE2_SIZE b = ov[2 * i + 1];
        dst[2 * i] = a == PCRE2_UNSET ? -1 : (int32_t)a;
        dst[2 * i + 1] = b == PCRE2_UNSET ? -1 : (int32_t)b;
    }
    pcre2_match_data_free(md);
    return out;
}

int32_t regex_test(int64_t h, int32_t input) {
    int32_t g = regex_find(h, input, 0);
    int32_t n = g ? ((int32_t *)(uintptr_t)(uint32_t)g)[0] : 0;
    free_tagged(g);
    return n > 0;
}

void regex_wasm_unused_tagged(void) {
    (void)malloc_tagged;
    (void)free_tagged;
}
