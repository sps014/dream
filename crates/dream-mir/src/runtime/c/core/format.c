#include "dream_core.h"
#include "dream_platform_internal.h"

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
    char buf[2];
    buf[0] = (char)(v >= 0 && v < 128 ? v : '?');
    buf[1] = 0;
    return from_utf8(buf);
}

/* Copies s (NUL-terminated) to o, returns one past the last byte written. Local so the guest
 * never grows an `env.strcpy` host import. */
static char *put_str(char *o, const char *s) {
    while (*s != '\0') {
        *o++ = *s++;
    }
    return o;
}

/* `%g` layout of the significant digits `d[0..n)` (n >= 1, no trailing zeros) of a value
 * d[0].d[1]... x 10^e: scientific when e < -4 or e >= 6, fixed otherwise. Returns one past the
 * last byte written. */
static char *put_g_digits(char *o, const int *d, int n, int e) {
    int i;
    if (e < -4 || e >= 6) {
        *o++ = (char)('0' + d[0]);
        if (n > 1) {
            *o++ = '.';
            for (i = 1; i < n; i++) {
                *o++ = (char)('0' + d[i]);
            }
        }
        *o++ = 'e';
        *o++ = e < 0 ? '-' : '+';
        int mag = e < 0 ? -e : e;
        if (mag >= 100) {
            *o++ = (char)('0' + mag / 100);
            mag %= 100;
        }
        *o++ = (char)('0' + mag / 10);
        *o++ = (char)('0' + mag % 10);
    } else if (e >= 0) {
        for (i = 0; i <= e; i++) {
            *o++ = (char)('0' + (i < n ? d[i] : 0));
        }
        if (n > e + 1) {
            *o++ = '.';
            for (i = e + 1; i < n; i++) {
                *o++ = (char)('0' + d[i]);
            }
        }
    } else {
        *o++ = '0';
        *o++ = '.';
        for (i = -1; i > e; i--) {
            *o++ = '0';
        }
        for (i = 0; i < n; i++) {
            *o++ = (char)('0' + d[i]);
        }
    }
    return o;
}

/* C `%g` with precision 6: up to six significant digits, trailing zeros stripped, scientific
 * notation when the exponent is < -4 or >= 6 (`50` not `50.0000`, `1e+06` not `1000000`). The
 * printf machinery this replaces is enormous once LTO inlines it into every caller, so the
 * wasm32 module only carries a few hundred bytes instead of the whole vsnprintf engine.
 * Deliberately libm-free (no fabs/log10/pow imports — wasm modules must link self-contained). */
static void format_g(double v, char *out) {
    double a = v < 0.0 ? -v : v;
    if (a != a) {
        *put_str(out, "nan") = 0;
        return;
    }
    /* Anything above DBL_MAX is infinity. */
    if (a > 1.7976931348623157081e+308) {
        *put_str(out, v < 0 ? "-inf" : "inf") = 0;
        return;
    }
    char *o = out;
    if (v < 0.0 && a != 0.0) {
        *o++ = '-';
    }
    if (a == 0.0) {
        *o++ = '0';
        *o = 0;
        return;
    }

    /* Decimal exponent via stride-16 / stride-1 scaling; a lands in [1, 10). */
    int e = 0;
    while (a >= 1e16) {
        a *= 1e-16;
        e += 16;
    }
    while (a >= 10.0) {
        a *= 0.1;
        e++;
    }
    while (a < 1e-15) {
        a *= 1e15;
        e -= 15;
    }
    while (a < 1.0) {
        a *= 10.0;
        e--;
    }

    int digits[6];
    uint64_t d = (uint64_t)(a * 100000.0 + 0.5);
    if (d >= 1000000u) {
        d /= 10u;
        e++;
    }
    int i;
    for (i = 5; i >= 0; i--) {
        digits[i] = (int)(d % 10u);
        d /= 10u;
    }
    int n = 6;
    while (n > 1 && digits[n - 1] == 0) {
        n--;
    }
    *put_g_digits(o, digits, n, e) = 0;
}

/* 10^k for 0 <= k <= 22, the range in which every power of ten is an exact double. */
static double exact_pow10(int k) {
    double p = 1.0;
    while (k-- > 0) {
        p *= 10.0;
    }
    return p;
}

/* The binary16 value nearest to finite `a >= 0` (ties to even), widened to double; +inf from
 * 65520 up, where binary16 overflows. Exact: only power-of-two scaling and integer rounding. */
static double half_round(double a) {
    if (a >= 65520.0) {
        return __builtin_inf();
    }
    /* Subnormals and the first normal binade share the 2^-24 quantum; it doubles per binade. */
    double ulp = 5.9604644775390625e-08;
    double bound = 1.220703125e-04;
    while (a >= bound) {
        ulp *= 2.0;
        bound *= 2.0;
    }
    double q = a / ulp;
    double whole = (double)(uint64_t)q;
    double frac = q - whole;
    if (frac > 0.5 || (frac == 0.5 && ((uint64_t)whole & 1u))) {
        whole += 1.0;
    }
    return whole * ulp;
}

/* Writes the shortest decimal (`%g` layout) that reads back as the binary16 value `a` (finite,
 * > 0) under round-to-nearest-even. At each precision only the two decimals bracketing `a` can
 * round to it, so both are tried, nearest first. Five significant digits always suffice. */
static char *put_half_shortest(char *o, double a) {
    int e = 0;
    if (a >= 1.0) {
        while (a >= exact_pow10(e + 1)) {
            e++;
        }
    } else {
        e = -1;
        while (a * exact_pow10(-e) < 1.0) {
            e--;
        }
    }
    for (int prec = 1; prec <= 9; prec++) {
        int k = prec - 1 - e;
        double scaled = k >= 0 ? a * exact_pow10(k) : a / exact_pow10(-k);
        double lo = (double)(uint64_t)scaled;
        double below = scaled - lo, above = lo + 1.0 - scaled;
        double cand[2] = {lo, lo + 1.0};
        if (below > above || (below == above && ((uint64_t)lo & 1u))) {
            cand[0] = lo + 1.0;
            cand[1] = lo;
        }
        for (int c = 0; c < 2; c++) {
            if (cand[c] == 0.0) {
                continue;
            }
            double back = k >= 0 ? cand[c] / exact_pow10(k) : cand[c] * exact_pow10(-k);
            if (half_round(back) != a) {
                continue;
            }
            int digits[12];
            int n = 0;
            uint64_t m = (uint64_t)cand[c];
            int rev[12];
            while (m != 0) {
                rev[n++] = (int)(m % 10u);
                m /= 10u;
            }
            for (int i = 0; i < n; i++) {
                digits[i] = rev[n - 1 - i];
            }
            int exp10 = n - 1 - k;
            while (n > 1 && digits[n - 1] == 0) {
                n--;
            }
            return put_g_digits(o, digits, n, exp10);
        }
    }
    dream_platform_abort();
}

dream_ptr dream_float16_to_string(float v) {
    char buf[24];
    double x = (double)v;
    double a = x < 0.0 ? -x : x;
    char *o = buf;
    if (a != a) {
        o = put_str(o, "nan");
    } else if (a > 65504.0) {
        o = put_str(o, x < 0.0 ? "-inf" : "inf");
    } else if (a == 0.0) {
        *o++ = '0';
    } else {
        if (x < 0.0) {
            *o++ = '-';
        }
        o = put_half_shortest(o, a);
    }
    *o = 0;
    return from_utf8(buf);
}

dream_ptr dream_float_to_string(float v) {
    char buf[40];
    format_g((double)v, buf);
    return from_utf8(buf);
}

dream_ptr dream_double_to_string(double v) {
    char buf[40];
    format_g(v, buf);
    return from_utf8(buf);
}

void dream_sb_push_double(dream_ptr sb, double v) {
    char text[40];
    uint16_t units[40];
    int32_t n = 0;
    format_g(v, text);
    while (text[n]) {
        units[n] = (uint16_t)(unsigned char)text[n];
        n++;
    }
    dream_sb_push_units(sb, units, n);
}
