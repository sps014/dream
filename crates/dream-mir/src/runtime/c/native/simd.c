#include "include/dream_rt_native.h"

#include <math.h>

/* Vector<T> C ABI is a 16-byte buffer pointer. Calls return a TLS slot so
 * `l = simd_v128_load(...)` can overwrite a stack-buffer local without leaking. */

static _Thread_local unsigned char simd_tmp[8][16];
static _Thread_local unsigned simd_i;

static dream_ptr simd_slot(void) {
    unsigned i = simd_i++ & 7u;
    return (dream_ptr)(uintptr_t)simd_tmp[i];
}

int32_t simd_lane_count(void) { return 4; }

dream_ptr simd_v128_load(dream_ptr arr, int32_t off) {
    dream_ptr p = simd_slot();
    memcpy(dream_p(p), (char *)dream_p(arr) + 4 + (size_t)off * 4, 16);
    return p;
}

void simd_v128_store(dream_ptr v, dream_ptr dest, int32_t off) {
    memcpy((char *)dream_p(dest) + 4 + (size_t)off * 4, dream_p(v), 16);
}

dream_ptr simd_v128_splat(float v) {
    dream_ptr p = simd_slot();
    float *d = (float *)dream_p(p);
    d[0] = d[1] = d[2] = d[3] = v;
    return p;
}

static dream_ptr simd_bin(dream_ptr a, dream_ptr b, int op) {
    dream_ptr p = simd_slot();
    const float *x = (const float *)dream_p(a);
    const float *y = (const float *)dream_p(b);
    float *d = (float *)dream_p(p);
    int i;
    for (i = 0; i < 4; i++) {
        float u = x[i];
        float v = y[i];
        if (op == 1) {
            d[i] = u - v;
        } else if (op == 2) {
            d[i] = u * v;
        } else if (op == 3) {
            d[i] = fminf(u, v);
        } else if (op == 4) {
            d[i] = fmaxf(u, v);
        } else {
            d[i] = u + v;
        }
    }
    return p;
}

dream_ptr simd_v128_add(dream_ptr a, dream_ptr b) { return simd_bin(a, b, 0); }
dream_ptr simd_v128_sub(dream_ptr a, dream_ptr b) { return simd_bin(a, b, 1); }
dream_ptr simd_v128_mul(dream_ptr a, dream_ptr b) { return simd_bin(a, b, 2); }
dream_ptr simd_v128_min(dream_ptr a, dream_ptr b) { return simd_bin(a, b, 3); }
dream_ptr simd_v128_max(dream_ptr a, dream_ptr b) { return simd_bin(a, b, 4); }

float simd_v128_sum(dream_ptr v) {
    const float *x = (const float *)dream_p(v);
    return x[0] + x[1] + x[2] + x[3];
}
