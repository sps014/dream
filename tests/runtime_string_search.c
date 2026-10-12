#include <assert.h>
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>

static int dream_units_equal(const void *a, const void *b, size_t bytes) {
    return !memcmp(a, b, bytes);
}
#include "../crates/dream-mir/src/runtime/c/core/string_search.h"

static int32_t reference(const uint16_t *h, size_t hn, const uint16_t *n, size_t nn) {
    if (nn > hn) { return -1; }
    for (size_t i = 0; i <= hn - nn; i++) {
        size_t j = 0;
        while (j < nn && h[i+j] == n[j]) { j++; }
        if (j == nn) { return (int32_t)i; }
    }
    return -1;
}
static void check(const uint16_t *h, size_t hn, const uint16_t *n, size_t nn) {
    assert(nn > 0 && nn <= hn);
    assert(find_long_needle(h, hn, n, nn) == reference(h, hn, n, nn));
}
static uint32_t seed = 0xabc123;
static uint32_t random_word(void) {
    seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5;
    return seed;
}
int main(void) {
    uint16_t h[1024], n[512];
    for (size_t hn = 1; hn <= 9; hn++) {
        for (unsigned hv = 0; hv < (1u << hn); hv++) {
            for (size_t i = 0; i < hn; i++) { h[i] = (hv >> i) & 1; }
            for (size_t nn = 1; nn <= hn; nn++) {
                for (unsigned nv = 0; nv < (1u << nn); nv++) {
                    for (size_t i = 0; i < nn; i++) { n[i] = (nv >> i) & 1; }
                    check(h, hn, n, nn);
                }
            }
        }
    }
    const uint16_t alphabet[] = {0, 1, 'a', 'b', 0xd800, 0xdc00, 0xffff};
    for (unsigned round = 0; round < 100000; round++) {
        size_t hn = 32 + random_word() % 993;
        size_t nn = 1 + random_word() % (hn < 512 ? hn : 512);
        unsigned domain = round % 3 == 0 ? 2 : sizeof(alphabet) / sizeof(alphabet[0]);
        for (size_t i = 0; i < hn; i++) { h[i] = alphabet[random_word() % domain]; }
        for (size_t i = 0; i < nn; i++) { n[i] = alphabet[random_word() % domain]; }
        if (round % 2) {
            size_t at = random_word() % (hn - nn + 1);
            memcpy(h + at, n, nn * sizeof(*n));
        }
        check(h, hn, n, nn);
        size_t offset = random_word() % (hn - nn + 1);
        check(h + offset, hn - offset, n, nn);
    }
    size_t hn = 1024 * 1024, nn = 4096;
    uint16_t *long_h = malloc(hn * sizeof(*long_h));
    uint16_t *long_n = malloc(nn * sizeof(*long_n));
    assert(long_h && long_n);
    for (size_t i = 0; i < hn; i++) { long_h[i] = 'a'; }
    for (size_t i = 0; i < nn; i++) { long_n[i] = 'a'; }
    long_n[nn-1] = 'b';
    assert(find_long_needle(long_h, hn, long_n, nn) == -1);
    long_h[hn-1] = 'b';
    assert(find_long_needle(long_h, hn, long_n, nn) == (int32_t)(hn-nn));
    /* A periodic factorization must preserve memory across overlapping candidates. */
    for (size_t i = 0; i < hn; i++) { long_h[i] = (uint16_t)(i & 1); }
    for (size_t i = 0; i < nn; i++) { long_n[i] = (uint16_t)(i & 1); }
    long_h[0] = 2;
    assert(find_long_needle(long_h, hn, long_n, nn) == 2);
    free(long_n); free(long_h);
}
