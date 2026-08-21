#include "include/dream_rt_native.h"

/* Debug live-object counter (defined in heap.c); immortals leave the count. */
extern int32_t live_objects;

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
    dream_ptr p = dream_malloc((int32_t)((size_t)n * 2 + 8), TAG_STRING);
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

/* Interned `"true"` / `"false"` singletons, pinned immortal (rc == INT32_MAX) because
 * callers release them through ordinary ARC. */
static dream_ptr bool_intern[2];

static void pin_immortal_obj(dream_ptr s);

/* Mark a shared singleton immortal: rc == INT32_MAX is ignored by retain/release, and
 * the block leaves Debug.live_objects accounting (it will never be freed). */
static void pin_immortal_obj(dream_ptr s) {
    if (s) {
        ((int32_t *)dream_p(s))[-1] = INT32_MAX;
        if (live_objects > 0) {
            live_objects -= 1;
        }
    }
}

static dream_ptr intern_bool(int32_t v) {
    dream_ptr slot = bool_intern[v ? 1 : 0];
    if (!slot) {
        dream_ptr s = from_utf8(v ? "true" : "false");
        pin_immortal_obj(s);
        bool_intern[v ? 1 : 0] = s;
        return s;
    }
    return slot;
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
 * never grows an `env.strcpy` host import (`libc.c` deliberately ships almost no libc). */
static char *put_str(char *o, const char *s) {
    while (*s != '\0') {
        *o++ = *s++;
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
        put_str(out, "nan");
        return;
    }
    /* Anything above DBL_MAX is infinity. */
    if (a > 1.7976931348623157081e+308) {
        put_str(out, v < 0 ? "-inf" : "inf");
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

    if (e < -4 || e >= 6) {
        *o++ = (char)('0' + digits[0]);
        int last = 5;
        while (last > 0 && digits[last] == 0) {
            last--;
        }
        if (last > 0) {
            *o++ = '.';
            for (i = 1; i <= last; i++) {
                *o++ = (char)('0' + digits[i]);
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
    } else {
        /* Fixed style: place the decimal point inside the six digits, zero-pad when the
         * value is smaller than 1, then strip trailing zeros / point. */
        if (e >= 0) {
            for (i = 0; i <= e; i++) {
                *o++ = (char)('0' + digits[i]);
            }
            int last = 5;
            while (last > e && digits[last] == 0) {
                last--;
            }
            if (last > e) {
                *o++ = '.';
                for (i = e + 1; i <= last; i++) {
                    *o++ = (char)('0' + digits[i]);
                }
            }
        } else {
            *o++ = '0';
            *o++ = '.';
            for (i = -1; i > e; i--) {
                *o++ = '0';
            }
            int last = 5;
            while (last > 0 && digits[last] == 0) {
                last--;
            }
            for (i = 0; i <= last; i++) {
                *o++ = (char)('0' + digits[i]);
            }
        }
    }
    *o = 0;
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
