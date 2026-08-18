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
    u = (uint16_t *)((char *)dream_p(p) + STRING_UTF8_OFFSET);
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

int main(void) {
    const int iters = 20000;
    dream_ptr s;
    volatile int32_t acc = 0;
    uint64_t t0;
    uint64_t t1;
    int i;
    int j;
    char buf[4096];
    memset(buf, 'a', sizeof(buf) - 1);
    buf[sizeof(buf) - 1] = 0;
    for (i = 0; i < (int)sizeof(buf) - 1; i += 7) {
        buf[i] = 'z';
    }
    s = mk_ascii(buf);

    t0 = ns_now();
    for (j = 0; j < iters; j++) {
        int32_t n = dream_str_len(s);
        for (i = 0; i < n; i++) {
            acc += dream_char_at_u(s, i);
        }
    }
    t1 = ns_now();
    printf("bench char_scan ns_per_op=%llu acc=%d\n", (unsigned long long)((t1 - t0) / iters), acc);

    t0 = ns_now();
    for (j = 0; j < iters; j++) {
        dream_ptr sub = dream_substring(s, 10, 200);
        acc += dream_str_len(sub);
        dream_release(sub);
    }
    t1 = ns_now();
    printf("bench substring ns_per_op=%llu acc=%d\n", (unsigned long long)((t1 - t0) / iters), acc);

    t0 = ns_now();
    for (j = 0; j < iters; j++) {
        dream_ptr c = dream_concat_strings(s, s);
        acc += dream_str_len(c);
        dream_release(c);
    }
    t1 = ns_now();
    printf("bench string_concat ns_per_op=%llu acc=%d\n", (unsigned long long)((t1 - t0) / iters),
           acc);
    return acc == 0 ? 1 : 0;
}
