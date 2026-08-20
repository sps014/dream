#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>

#include "dream_rt_wasm32.h"

void *memcpy(void *dst, const void *src, size_t n) {
    uint8_t *d = (uint8_t *)dst;
    const uint8_t *s = (const uint8_t *)src;
    size_t i;
    for (i = 0; i < n; i++) {
        d[i] = s[i];
    }
    return dst;
}

void *memset(void *dst, int c, size_t n) {
    uint8_t *d = (uint8_t *)dst;
    size_t i;
    for (i = 0; i < n; i++) {
        d[i] = (uint8_t)c;
    }
    return dst;
}

void *memmove(void *dst, const void *src, size_t n) {
    uint8_t *d = (uint8_t *)dst;
    const uint8_t *s = (const uint8_t *)src;
    size_t i;
    if (d == s || n == 0) {
        return dst;
    }
    if (d < s) {
        return memcpy(dst, src, n);
    }
    i = n;
    while (i) {
        i--;
        d[i] = s[i];
    }
    return dst;
}

int memcmp(const void *a, const void *b, size_t n) {
    const uint8_t *x = (const uint8_t *)a;
    const uint8_t *y = (const uint8_t *)b;
    size_t i;
    for (i = 0; i < n; i++) {
        if (x[i] != y[i]) {
            return (int)x[i] - (int)y[i];
        }
    }
    return 0;
}

void abort(void) {
    __builtin_trap();
}

size_t strlen(const char *s) {
    size_t n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

void *malloc(size_t n) {
    if (n > (size_t)INT32_MAX) {
        return NULL;
    }
    return dream_p(dream_malloc((int32_t)n, 0));
}

void *calloc(size_t n, size_t sz) {
    size_t bytes;
    void *p;
    if (sz != 0 && n > (size_t)INT32_MAX / sz) {
        return NULL;
    }
    bytes = n * sz;
    p = malloc(bytes);
    if (p) {
        memset(p, 0, bytes);
    }
    return p;
}

void free(void *p) {
    if (p) {
        dream_free((dream_ptr)(uintptr_t)p);
    }
}

static void append_char(char *dst, size_t cap, size_t *o, char c) {
    if (*o + 1 < cap) {
        dst[*o] = c;
    }
    *o += 1;
}

static void append_str(char *dst, size_t cap, size_t *o, const char *s) {
    if (!s) {
        s = "(null)";
    }
    while (*s) {
        append_char(dst, cap, o, *s++);
    }
}

static void append_u64(char *dst, size_t cap, size_t *o, uint64_t v, int hex) {
    char buf[32];
    int n = 0;
    uint64_t base = hex ? 16u : 10u;
    if (v == 0) {
        append_char(dst, cap, o, '0');
        return;
    }
    while (v) {
        uint64_t d = v % base;
        buf[n++] = (char)(d < 10 ? '0' + d : 'a' + (d - 10));
        v /= base;
    }
    while (n) {
        append_char(dst, cap, o, buf[--n]);
    }
}

int vsnprintf(char *dst, size_t cap, const char *fmt, va_list ap) {
    size_t o = 0;
    if (!fmt) {
        if (cap) {
            dst[0] = 0;
        }
        return 0;
    }
    while (*fmt) {
        if (*fmt != '%') {
            append_char(dst, cap, &o, *fmt++);
            continue;
        }
        fmt++;
        if (*fmt == '%') {
            append_char(dst, cap, &o, '%');
            fmt++;
            continue;
        }
        while (*fmt == 'l' || *fmt == 'z' || *fmt == 'j' || *fmt == 't') {
            fmt++;
        }
        switch (*fmt) {
        case 's':
            append_str(dst, cap, &o, va_arg(ap, const char *));
            break;
        case 'd':
        case 'i': {
            int v = va_arg(ap, int);
            if (v < 0) {
                append_char(dst, cap, &o, '-');
                append_u64(dst, cap, &o, (uint64_t)(-(int64_t)v), 0);
            } else {
                append_u64(dst, cap, &o, (uint64_t)v, 0);
            }
            break;
        }
        case 'u':
            append_u64(dst, cap, &o, (uint64_t)va_arg(ap, unsigned), 0);
            break;
        case 'x':
            append_u64(dst, cap, &o, (uint64_t)va_arg(ap, unsigned), 1);
            break;
        case 'p':
            append_str(dst, cap, &o, "0x");
            append_u64(dst, cap, &o, (uint64_t)(uintptr_t)va_arg(ap, void *), 1);
            break;
        case 'c':
            append_char(dst, cap, &o, (char)va_arg(ap, int));
            break;
        case 'g':
        case 'f':
        case 'e': {
            double v = va_arg(ap, double);
            int64_t ip;
            int is_g = *fmt == 'g';
            if (v < 0) {
                append_char(dst, cap, &o, '-');
                v = -v;
            }
            ip = (int64_t)v;
            append_u64(dst, cap, &o, (uint64_t)ip, 0);
            append_char(dst, cap, &o, '.');
            {
                uint64_t frac = (uint64_t)((v - (double)ip) * 1000000.0 + 0.5);
                char fbuf[8];
                int i;
                int n = 6;
                for (i = 5; i >= 0; i--) {
                    fbuf[i] = (char)('0' + (frac % 10));
                    frac /= 10;
                }
                if (is_g) {
                    while (n > 0 && fbuf[n - 1] == '0') {
                        n--;
                    }
                }
                if (n == 0) {
                    o -= 1;
                } else {
                    for (i = 0; i < n; i++) {
                        append_char(dst, cap, &o, fbuf[i]);
                    }
                }
            }
            break;
        }
        default:
            append_char(dst, cap, &o, '%');
            if (*fmt) {
                append_char(dst, cap, &o, *fmt);
            }
            break;
        }
        if (*fmt) {
            fmt++;
        }
    }
    if (cap) {
        dst[o < cap ? o : cap - 1] = 0;
    }
    return (int)o;
}

int snprintf(char *dst, size_t cap, const char *fmt, ...) {
    va_list ap;
    int n;
    va_start(ap, fmt);
    n = vsnprintf(dst, cap, fmt, ap);
    va_end(ap);
    return n;
}
