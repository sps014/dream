#include "../crates/dream-mir/src/runtime/c/core/include/dream_core.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>

static int fail_mapping;
static int fail_counters;
#include "dream_platform_internal.h"
static void *test_allocate(size_t size) {
    return fail_counters ? NULL : dream_default_platform.allocate(size);
}
static void *test_map(size_t size) {
    return fail_mapping ? NULL : dream_default_platform.map(size);
}
#include "../crates/dream-mir/src/runtime/c/core/heap.c"

void dream_future_fini(dream_ptr ptr) { (void)ptr; }
void dream_panic(dream_ptr message) {
    /* A static diagnostic must not register counters or allocate a Dream block. */
    assert(counters_head == NULL);
    for (int i = 0; i < dream_str_len(message); ++i) {
        fputc(dream_char_at_u(message, i), stderr);
    }
    fputc('\n', stderr);
    exit(86);
}

int main(int argc, char **argv) {
    dream_platform platform = dream_default_platform;
    platform.allocate = test_allocate;
    platform.map = test_map;
    dream_set_platform(&platform);
    alarm(5);
    assert(argc == 2);
    if (strcmp(argv[1], "private-size") == 0) { dream_malloc_slow(SIZE_MAX, 0); }
    if (strcmp(argv[1], "shared-size") == 0) { dream_malloc_shared(-1, 0); }
    if (strcmp(argv[1], "realloc-size") == 0) { dream_realloc(0, SIZE_MAX, 0); }
    if (strcmp(argv[1], "array-size") == 0) { dream_array_realloc(0, -1, 8); }
    if (strcmp(argv[1], "string-size") == 0) { dream_string_bytes(-1); }
    if (strcmp(argv[1], "string-byte-count") == 0) {
        int32_t string[] = {INT32_MAX, 0};
        dream_str_byte_size((dream_ptr)string);
    }
    if (strcmp(argv[1], "string-count") == 0) { dream_string_count_add(INT32_MAX, 1); }
    if (strcmp(argv[1], "js") == 0) { dream_js_call(0, 0, 0, 0); }
    if (strcmp(argv[1], "counters") == 0) {
        fail_counters = 1;
        dream_malloc_slow(16, 0);
    }
    fail_mapping = 1;
    if (strcmp(argv[1], "shared-map") == 0) { dream_malloc_shared(16, 0); }
    /* Exercise mmap failure without first allocating a per-thread counter. */
    tls_bump(16);
    return 1;
}

void dream_release_object(dream_ptr ptr) { dream_release(ptr); }
