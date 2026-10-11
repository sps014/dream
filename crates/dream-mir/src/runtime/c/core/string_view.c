/* `ReadOnlySpan<char>` helpers. Every view is `(string, offset, length)` in UTF-16 units; the Dream
 * side clamps, so `offset + length <= dream_str_len(s)` holds on entry. */
#include "dream_core.h"

#include <string.h>

static const uint16_t *view_units(dream_ptr s, int32_t off) {
    const uint16_t *u = dream_str_units(s);
    return u ? u + off : NULL;
}

typedef uint16_t u16x8 __attribute__((vector_size(16)));
typedef int16_t i16x8 __attribute__((vector_size(16)));
typedef _Bool bool8 __attribute__((ext_vector_type(8)));

/* Eight units per compare, lowered to SSE2/NEON/simd128 under the program's own CPU (the bitcode
 * has no target features). Out of line so it does not bloat loops that inline `find_unit`. */
__attribute__((noinline)) static int32_t find_unit_wide(const uint16_t *u, int32_t i, int32_t n,
                                                        uint16_t c) {
    u16x8 needle = (u16x8){0} + c;
    for (; i + 8 <= n; i += 8) {
        u16x8 v;
        uint8_t bits;
        memcpy(&v, u + i, sizeof(v));
        bool8 hit = __builtin_convertvector((i16x8)(v == needle), bool8);
        memcpy(&bits, &hit, sizeof(bits));
        if (bits != 0) {
            return i + __builtin_ctz(bits);
        }
    }
    for (; i < n; i++) {
        if (u[i] == c) {
            return i;
        }
    }
    return -1;
}

/* First `i` in `[from, n)` with `u[i] == c`, or -1. Separators in split-heavy text are usually
 * a few units apart, so the first eight are scanned one by one before going wide. */
static int32_t find_unit(const uint16_t *u, int32_t from, int32_t n, uint16_t c) {
    int32_t head = n - from < 8 ? n : from + 8;
    for (int32_t i = from; i < head; i++) {
        if (u[i] == c) {
            return i;
        }
    }
    return head < n ? find_unit_wide(u, head, n, c) : -1;
}

int32_t string_view_eq(dream_ptr a, int32_t ao, int32_t an, dream_ptr b, int32_t bo, int32_t bn) {
    if (an != bn) {
        return 0;
    }
    if (an <= 0 || (a == b && ao == bo)) {
        return 1;
    }
    return dream_units_equal(view_units(a, ao), view_units(b, bo), (size_t)an << 1);
}

int32_t string_view_compare(dream_ptr a, int32_t ao, int32_t an, dream_ptr b, int32_t bo,
                            int32_t bn) {
    int32_t n = an < bn ? an : bn;
    int32_t i;
    const uint16_t *ua = view_units(a, ao);
    const uint16_t *ub = view_units(b, bo);
    for (i = 0; i < n; i++) {
        if (ua[i] != ub[i]) {
            return (int32_t)ua[i] - (int32_t)ub[i];
        }
    }
    return an - bn;
}

/* Equal to `dream_string_hash` of the same units, so a span finds its `string` key in a Map. */
int32_t string_view_hash(dream_ptr s, int32_t off, int32_t n) {
    uint32_t hash = 2166136261u;
    int32_t i;
    const uint16_t *u;
    if (off == 0 && n == dream_str_len(s)) {
        return dream_string_hash(s);
    }
    u = view_units(s, off);
    for (i = 0; i < n; i++) {
        hash ^= u[i];
        hash *= 16777619u;
    }
    if (hash <= 1u) {
        hash += 2u;
    }
    return (int32_t)hash;
}

/* First index `>= from` (relative to the haystack view) where the needle view occurs, or -1. */
int32_t string_view_find(dream_ptr h, int32_t ho, int32_t hn, dream_ptr nd, int32_t no,
                         int32_t nn, int32_t from) {
    const uint16_t *hu;
    const uint16_t *nu;
    int32_t i;
    if (from < 0) {
        from = 0;
    }
    if (nn <= 0) {
        return from <= hn ? from : -1;
    }
    if (nn > hn) {
        return -1;
    }
    hu = view_units(h, ho);
    nu = view_units(nd, no);
    for (i = from; i <= hn - nn; i++) {
        i = find_unit(hu, i, hn - nn + 1, nu[0]);
        if (i < 0) {
            return -1;
        }
        if (dream_units_equal(hu + i, nu, (size_t)nn << 1)) {
            return i;
        }
    }
    return -1;
}

/* Last index where the needle view occurs, or -1. An empty needle matches at `hn`. */
int32_t string_view_rfind(dream_ptr h, int32_t ho, int32_t hn, dream_ptr nd, int32_t no,
                          int32_t nn) {
    const uint16_t *hu;
    const uint16_t *nu;
    int32_t i;
    if (nn <= 0) {
        return hn;
    }
    if (nn > hn) {
        return -1;
    }
    hu = view_units(h, ho);
    nu = view_units(nd, no);
    for (i = hn - nn; i >= 0; i--) {
        if (hu[i] == nu[0] && dream_units_equal(hu + i, nu, (size_t)nn << 1)) {
            return i;
        }
    }
    return -1;
}

int32_t string_view_find_char(dream_ptr s, int32_t off, int32_t n, int32_t from, int32_t c) {
    const uint16_t *u;
    if (from < 0) {
        from = 0;
    }
    if (from >= n) {
        return -1;
    }
    u = view_units(s, off);
    return find_unit(u, from, n, (uint16_t)c);
}

typedef int32_t i32x8 __attribute__((vector_size(32)));

/* Occurrences of `c` in the view, so `split` sizes its result without a find call per piece.
 * Each 16-bit lane counts at most 4096 blocks before it is widened, far below overflow. */
int32_t string_view_count_char(dream_ptr s, int32_t off, int32_t n, int32_t c) {
    const uint16_t *u;
    u16x8 needle = (u16x8){0} + (uint16_t)c;
    int32_t total = 0;
    int32_t i = 0;
    if (n <= 0) {
        return 0;
    }
    u = view_units(s, off);
    while (i + 8 <= n) {
        i16x8 lanes = {0};
        for (int32_t blocks = 0; blocks < 4096 && i + 8 <= n; blocks++, i += 8) {
            u16x8 v;
            memcpy(&v, u + i, sizeof(v));
            lanes -= (i16x8)(v == needle);
        }
        total += __builtin_reduce_add(__builtin_convertvector(lanes, i32x8));
    }
    for (; i < n; i++) {
        total += u[i] == (uint16_t)c;
    }
    return total;
}

int32_t string_view_rfind_char(dream_ptr s, int32_t off, int32_t n, int32_t c) {
    const uint16_t *u = view_units(s, off);
    int32_t i;
    for (i = n - 1; i >= 0; i--) {
        if (u[i] == (uint16_t)c) {
            return i;
        }
    }
    return -1;
}

/* A slice string over the first `len` payload bytes of a `StringBuilder` buffer (units start
 * after the array length word and the builder's 4-byte pad, as in `string_from_builder`). The
 * slice retains the buffer; the builder never rewrites viewed units in place (growth copies
 * into a new block, and `clear` swaps buffers once viewed), so the view is a stable snapshot. */
dream_ptr string_builder_view(dream_ptr bytes, int32_t len) {
    dream_ptr p;
    int32_t n = len >> 1;
    if (!bytes || n <= 0) {
        return dream_string_alloc(0);
    }
    p = dream_malloc(DREAM_SLICE_BYTES, TAG_STRING);
    dream_slice_fill(p, bytes, n, (const uint16_t *)((char *)dream_p(bytes) + 8));
    dream_slice_retain_parent(bytes);
    return p;
}
