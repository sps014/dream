#include "include/dream_rt_native.h"

#include <stdio.h>
#include <time.h>

static dream_ptr mk_ascii(const char *s) {
    size_t n = strlen(s);
    dream_ptr p = dream_malloc((int32_t)(n * 2 + 8), TAG_STRING);
    uint16_t *u;
    size_t i;
    dream_i32(p)[0] = (int32_t)n;
    dream_i32(p)[1] = 0;
    u = (uint16_t *)((char *)dream_p(p) + STRING_UNITS_OFFSET);
    for (i = 0; i < n; i++) {
        u[i] = (uint16_t)(unsigned char)s[i];
    }
    return p;
}

static uint64_t ns_now(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

static void sb_count_reset(dream_ptr sb) { ((dream_sb *)dream_p(sb))->count = 0; }

/* Minimum of `reps` runs — single-shot timings on a loaded machine swing ±30%. */

/* Minimum of `reps` runs — single-shot timings swing ±30% on a loaded machine. */
#define BENCH_MIN(label, reps, pushes, body) \
    do { \
        double best = 1e30; \
        uint32_t rep_; \
        for (rep_ = 0; rep_ < (reps); rep_++) { \
            uint64_t t0_, t1_; \
            t0_ = ns_now(); \
            { body } \
            t1_ = ns_now(); \
            double el_ = (double)(t1_ - t0_) / (double)(pushes); \
            if (el_ < best) best = el_; \
        } \
        printf("bench " label " ns_per_op=%.2f acc=%d\n", best, acc); \
    } while (0)

int main(void) {
    const int iters = 20000;
    dream_ptr s;
    volatile int32_t acc = 0;
    int i;
    int j;
    char buf[4096];
    memset(buf, 'a', sizeof(buf) - 1);
    buf[sizeof(buf) - 1] = 0;
    for (i = 0; i < (int)sizeof(buf) - 1; i += 7) {
        buf[i] = 'z';
    }
    s = mk_ascii(buf);

    BENCH_MIN("char_scan", 5, iters, {
        int32_t n = dream_str_len(s);
        for (i = 0; i < n; i++) {
            acc += dream_char_at_u(s, i);
        }
    });

    BENCH_MIN("substring", 5, iters, {
        dream_ptr sub = dream_substring(s, 10, 200);
        acc += dream_str_len(sub);
        dream_release(sub);
    });

    BENCH_MIN("string_concat", 5, iters, {
        dream_ptr c = dream_concat_strings(s, s);
        acc += dream_str_len(c);
        dream_release(c);
    });

    {
        /* Steady-state literal appends into a pre-reserved builder (the microbench shape). */
        const int pushes = iters * 4;
        dream_ptr sb = dream_malloc(16, TAG_STRUCT_BASE);
        ((dream_sb *)dream_p(sb))->bytes = 0;
        ((dream_sb *)dream_p(sb))->count = 0;
        ((dream_sb *)dream_p(sb))->cap = 0;
        dream_sb_push_units(sb, "abcdefghijklmnopqrstuvwxyz", 26); /* warm paths */

        BENCH_MIN("sb_push_reserved", 5, pushes, {
            for (i = 0; i < pushes; i++) {
                dream_sb_push_units(sb, "abcdefghijklmnopqrstuvwxyz", 26);
            }
            sb_count_reset(sb);
        });

        /* Growing appends: exercise the realloc path every ~capacity doubling. */
        BENCH_MIN("sb_push_growing", 5, iters, {
            dream_ptr sb2 = dream_malloc(16, TAG_STRUCT_BASE);
            ((dream_sb *)dream_p(sb2))->bytes = 0;
            ((dream_sb *)dream_p(sb2))->count = 0;
            ((dream_sb *)dream_p(sb2))->cap = 0;
            for (i = 0; i < iters; i++) {
                dream_sb_push_units(sb2, "abcdefghijklmnopqrstuvwxyz", 26);
            }
            acc += dream_i32(((dream_sb *)dream_p(sb2))->bytes)[1];
            dream_release(((dream_sb *)dream_p(sb2))->bytes);
            dream_release(sb2);
        });

        dream_ptr built = string_from_builder(((dream_sb *)dream_p(sb))->bytes,
                                              ((dream_sb *)dream_p(sb))->count,
                                              ((dream_sb *)dream_p(sb))->count >> 1);
        acc += dream_str_len(built);
        dream_release(built);
        dream_release(sb);
    }
    return acc == 0 ? 1 : 0;
}
