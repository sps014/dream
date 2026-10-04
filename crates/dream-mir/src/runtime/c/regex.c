/* Shared PCRE2 wrapper for wasm32 (interpreter) and native (JIT via SUPPORT_JIT). */
#define PCRE2_CODE_UNIT_WIDTH 16
#define PCRE2_STATIC 1
#include "pcre2.h"
#include "dream_guest.h"

#include <stdlib.h>
#include <string.h>
#include <limits.h>

typedef struct {
    pcre2_code *code;
    uint32_t capture_count;
} DreamRe;

static uint32_t compile_opts(int32_t flags) {
    uint32_t opt = PCRE2_UTF;
    if (flags & DREAM_REGEX_IGNORE_CASE) {
        opt |= PCRE2_CASELESS;
    }
    if (flags & DREAM_REGEX_MULTILINE) {
        opt |= PCRE2_MULTILINE;
    }
    if (flags & DREAM_REGEX_DOTALL) {
        opt |= PCRE2_DOTALL;
    }
    return opt;
}

static DreamRe *as_re(uintptr_t h) {
    if (h == 0) {
        return NULL;
    }
    return (DreamRe *)(uintptr_t)(uint64_t)h;
}

uintptr_t regex_compile(dream_ptr pattern, int32_t flags) {
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
    n = dream_str_len(pattern);
    pat = (const PCRE2_UCHAR *)dream_str_units(pattern);
    code = pcre2_compile(pat, (PCRE2_SIZE)n, compile_opts(flags), &err, &err_off, NULL);
    if (code == NULL) {
        return 0;
    }
#ifdef SUPPORT_JIT
    (void)pcre2_jit_compile(code, PCRE2_JIT_COMPLETE);
#endif
    (void)pcre2_pattern_info(code, PCRE2_INFO_CAPTURECOUNT, &ncap);
    re = (DreamRe *)malloc(sizeof(DreamRe));
    if (re == NULL) {
        pcre2_code_free(code);
        return 0;
    }
    re->code = code;
    re->capture_count = ncap;
    return (uintptr_t)re;
}

void regex_free(uintptr_t h) {
    DreamRe *re = as_re(h);
    if (re == NULL) {
        return;
    }
    pcre2_code_free(re->code);
    free(re);
}

int32_t regex_group_count(uintptr_t h) {
    DreamRe *re = as_re(h);
    return re ? (int32_t)re->capture_count : 0;
}

int32_t regex_name_count(uintptr_t h) {
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

dream_ptr regex_name_at(uintptr_t h, int32_t i) {
    DreamRe *re = as_re(h);
    uint32_t es = 0;
    const PCRE2_UCHAR *ent;
    int32_t n = 0;
    if (re == NULL) {
        return dream_str_from_units(NULL, 0);
    }
    ent = name_entry(re, i, &es);
    if (ent == NULL || es < 2) {
        return dream_str_from_units(NULL, 0);
    }
    while (n + 1 < (int32_t)es && ent[1 + n] != 0) {
        n++;
    }
    return dream_str_from_units(ent + 1, n);
}

int32_t regex_name_number(uintptr_t h, int32_t i) {
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

dream_ptr regex_find(uintptr_t h, dream_ptr input, int32_t pos) {
    DreamRe *re = as_re(h);
    pcre2_match_data *md;
    int rc;
    int32_t n;
    int32_t pairs;
    int32_t i;
    dream_ptr out;
    int32_t *dst;
    PCRE2_SIZE *ov;
    const PCRE2_UCHAR *sub;
    if (re == NULL || !input) {
        return dream_array_i32(0);
    }
    n = dream_str_len(input);
    if (pos < 0) {
        pos = 0;
    }
    if (pos > n) {
        return dream_array_i32(0);
    }
    md = pcre2_match_data_create_from_pattern(re->code, NULL);
    if (md == NULL) {
        return dream_array_i32(0);
    }
    sub = (const PCRE2_UCHAR *)dream_str_units(input);
    rc = pcre2_match(re->code, sub, (PCRE2_SIZE)n, (PCRE2_SIZE)pos, 0, md, NULL);
    if (rc < 0) {
        pcre2_match_data_free(md);
        return dream_array_i32(0);
    }
    pairs = (int32_t)pcre2_get_ovector_count(md);
    ov = pcre2_get_ovector_pointer(md);
    out = dream_array_i32(pairs * 2);
    dst = dream_i32s(out) + 1;
    for (i = 0; i < pairs; i++) {
        PCRE2_SIZE a = ov[2 * i];
        PCRE2_SIZE b = ov[2 * i + 1];
        dst[2 * i] = a == PCRE2_UNSET ? -1 : (int32_t)a;
        dst[2 * i + 1] = b == PCRE2_UNSET ? -1 : (int32_t)b;
    }
    pcre2_match_data_free(md);
    return out;
}

dream_ptr regex_find_all(uintptr_t h, dream_ptr input) {
    DreamRe *re = as_re(h);
    int32_t local[32];
    int32_t *offsets = local;
    int32_t count = 0, capacity = 32;
    int32_t n = dream_str_len(input);
    PCRE2_SIZE pos = 0;
    uint32_t options = 0;
    pcre2_match_data *md;
    dream_ptr out;
    const PCRE2_UCHAR *subject;
    if (re == NULL || !input) { return dream_array_i32(0); }
    md = pcre2_match_data_create_from_pattern(re->code, NULL);
    if (md == NULL) { return dream_array_i32(0); }
    subject = (const PCRE2_UCHAR *)dream_str_units(input);
    while (pos <= (PCRE2_SIZE)n && pcre2_match(re->code, subject, (PCRE2_SIZE)n,
                                             pos, options, md, NULL) >= 0) {
        PCRE2_SIZE *ov = pcre2_get_ovector_pointer(md);
        if (count == capacity) {
            int32_t next;
            int32_t *grown;
            if (capacity > (INT32_MAX / 4 - 4) / 2) { count = 0; break; }
            next = capacity * 2;
            grown = (int32_t *)malloc((size_t)next * sizeof(*grown));
            if (grown == NULL) { count = 0; break; }
            memcpy(grown, offsets, (size_t)count * sizeof(*grown));
            if (offsets != local) { free(offsets); }
            offsets = grown;
            capacity = next;
        }
        offsets[count++] = (int32_t)ov[0];
        offsets[count++] = (int32_t)ov[1];
        /* The first successful search validates the entire UTF-16 subject. */
        options = PCRE2_NO_UTF_CHECK;
        if (ov[1] > ov[0]) {
            pos = ov[1];
        } else {
            pos = ov[0] + 1;
            if (pos < (PCRE2_SIZE)n && subject[pos - 1] >= 0xd800 &&
                subject[pos - 1] <= 0xdbff && subject[pos] >= 0xdc00 &&
                subject[pos] <= 0xdfff) { ++pos; }
        }
    }
    out = dream_array_i32(count);
    if (count) { memcpy(dream_i32s(out) + 1, offsets, (size_t)count * sizeof(*offsets)); }
    if (offsets != local) { free(offsets); }
    pcre2_match_data_free(md);
    return out;
}

int32_t regex_test(uintptr_t h, dream_ptr input) {
    dream_ptr g = regex_find(h, input, 0);
    int32_t n = dream_array_len(g);
    dream_drop(g);
    return n > 0;
}
