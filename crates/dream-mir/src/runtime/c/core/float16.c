#include "dream_core.h"

/* LLVM lowers `half` conversions on targets without native binary16 arithmetic to these
 * compiler-rt builtins. Windows links guests with lld-link and no compiler-rt, so the runtime
 * supplies them there; every other target already gets them from compiler-rt or libgcc.
 * Narrowing rounds directly from the source width (to nearest, ties to even), so `float` and
 * `double` sources are each rounded exactly once. */
#if defined(_WIN32)

static uint32_t dream_half_to_float_bits(uint16_t h) {
    uint32_t sign = (uint32_t)(h & 0x8000u) << 16;
    uint32_t exp = (h >> 10) & 0x1fu;
    uint32_t man = h & 0x3ffu;
    if (exp == 0) {
        if (man == 0) {
            return sign;
        }
        exp = 127 - 15 + 1;
        while ((man & 0x400u) == 0) {
            man <<= 1;
            exp--;
        }
        return sign | (exp << 23) | ((man & 0x3ffu) << 13);
    }
    if (exp == 31) {
        return sign | 0x7f800000u | (man << 13);
    }
    return sign | ((exp + 112) << 23) | (man << 13);
}

static uint64_t dream_round_shift(uint64_t value, int shift) {
    uint64_t quotient = value >> shift;
    uint64_t remainder = value & ((1ull << shift) - 1);
    uint64_t half = 1ull << (shift - 1);
    if (remainder > half || (remainder == half && (quotient & 1))) {
        quotient++;
    }
    return quotient;
}

static uint16_t dream_double_to_half_bits(uint64_t d) {
    uint16_t sign = (uint16_t)((d >> 48) & 0x8000u);
    int exp = (int)((d >> 52) & 0x7ff);
    uint64_t man = d & 0xfffffffffffffull;
    if (exp == 0x7ff) {
        return man ? (uint16_t)(sign | 0x7e00u | (man >> 42)) : (uint16_t)(sign | 0x7c00u);
    }
    if (exp == 0) {
        return sign;
    }
    int e = exp - 1023 + 15;
    if (e >= 31) {
        return (uint16_t)(sign | 0x7c00u);
    }
    if (e <= 0) {
        /* Subnormal half: units of 2^-24 are (1.m * 2^(exp-1023)) / 2^-24. */
        int shift = 1051 - exp;
        if (shift > 54) {
            return sign;
        }
        return (uint16_t)(sign | dream_round_shift(man | (1ull << 52), shift));
    }
    /* A mantissa carry into the exponent is the correctly rounded result, including overflow
     * of e = 30 into infinity. */
    return (uint16_t)(sign | (((uint64_t)e << 10) + dream_round_shift(man, 42)));
}

float __extendhfsf2(_Float16 a) {
    uint16_t h;
    __builtin_memcpy(&h, &a, sizeof h);
    uint32_t bits = dream_half_to_float_bits(h);
    float f;
    __builtin_memcpy(&f, &bits, sizeof f);
    return f;
}

double __extendhfdf2(_Float16 a) {
    return (double)__extendhfsf2(a);
}

_Float16 __truncdfhf2(double a) {
    uint64_t bits;
    __builtin_memcpy(&bits, &a, sizeof bits);
    uint16_t h = dream_double_to_half_bits(bits);
    _Float16 result;
    __builtin_memcpy(&result, &h, sizeof result);
    return result;
}

_Float16 __truncsfhf2(float a) {
    return __truncdfhf2((double)a);
}

#endif
