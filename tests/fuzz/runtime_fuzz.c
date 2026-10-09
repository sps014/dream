/* libFuzzer entry for the portable runtime's text paths: UTF-8 <-> UTF-16 conversion, float
 * formatting, and view search. The first input byte picks the target; the rest is its input.
 * Build and run through `scripts/fuzz_runtime.sh`. */
#include "../../crates/dream-mir/src/runtime/c/core/include/dream_core.h"
#include <stdio.h>
#include <stdlib.h>

int32_t string_view_find(dream_ptr h, int32_t ho, int32_t hn, dream_ptr nd, int32_t no,
                         int32_t nn, int32_t from);
dream_ptr string_from_utf8(dream_ptr bytes);
dream_ptr dream_double_to_string(double v);

void dream_future_fini(dream_ptr ptr) { (void)ptr; }
void dream_release_object(dream_ptr ptr) { dream_release(ptr); }

void dream_panic(dream_ptr message) {
    (void)message;
    fputs("runtime panic\n", stderr);
    abort();
}

const dream_type_info *dream_type_info_for_tag(int32_t tag) { return dream_builtin_type_info(tag); }

static dream_ptr units_string(const uint8_t *data, size_t n) {
    int32_t units = (int32_t)(n / 2);
    dream_ptr s = dream_string_alloc(units);
    if (units > 0) {
        memcpy((char *)dream_p(s) + STRING_UNITS_OFFSET, data, (size_t)units * 2);
    }
    return s;
}

static void fuzz_utf8(const uint8_t *data, size_t n) {
    dream_ptr bytes = dream_array_new((int32_t)n, 1);
    memcpy((char *)dream_p(bytes) + 4, data, n);
    dream_ptr text = string_from_utf8(bytes);
    char *back = dream_string_to_utf8(text);
    if (back != NULL) {
        free(back);
    }
    dream_release(text);
    dream_release(bytes);
}

static void fuzz_format(const uint8_t *data, size_t n) {
    double v = 0;
    memcpy(&v, data, n < sizeof(v) ? n : sizeof(v));
    dream_ptr s = dream_double_to_string(v);
    if (dream_str_len(s) <= 0 || dream_str_len(s) > 64) {
        abort();
    }
    dream_release(s);
}

static int32_t naive_find(const uint16_t *h, int32_t hn, const uint16_t *nd, int32_t nn, int32_t from) {
    if (from < 0) {
        from = 0;
    }
    if (nn <= 0) {
        return from <= hn ? from : -1;
    }
    for (int32_t i = from; i + nn <= hn; i++) {
        if (memcmp(h + i, nd, (size_t)nn * 2) == 0) {
            return i;
        }
    }
    return -1;
}

static void fuzz_find(const uint8_t *data, size_t n) {
    if (n < 2) {
        return;
    }
    size_t split = data[0] % (n - 1) + 1;
    int32_t from = (int32_t)(data[1] % 8) - 2;
    dream_ptr h = units_string(data + 2, split > 2 ? split - 2 : 0);
    dream_ptr nd = units_string(data + split, n - split > 16 ? 16 : n - split);
    int32_t hn = dream_str_len(h);
    int32_t nn = dream_str_len(nd);
    int32_t got = string_view_find(h, 0, hn, nd, 0, nn, from);
    int32_t want = naive_find(dream_str_units(h), hn, dream_str_units(nd), nn, from);
    if (got != want) {
        fprintf(stderr, "string_view_find mismatch: got %d want %d\n", got, want);
        abort();
    }
    dream_release(h);
    dream_release(nd);
}

int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) {
    if (size == 0) {
        return 0;
    }
    switch (data[0] % 3) {
    case 0: fuzz_utf8(data + 1, size - 1); break;
    case 1: fuzz_format(data + 1, size - 1); break;
    default: fuzz_find(data + 1, size - 1); break;
    }
    return 0;
}
