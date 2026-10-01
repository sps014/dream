#include "../crates/dream-mir/src/runtime/c/native/include/dream_rt_native.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>

static int fail_mapping;
static int fail_counters;
static void *test_calloc(size_t count, size_t size) {
    return fail_counters ? NULL : calloc(count, size);
}
static void *test_mmap(void *address, size_t size, int protection, int flags,
                       int fd, off_t offset) {
    return fail_mapping ? MAP_FAILED : mmap(address, size, protection, flags, fd, offset);
}
#define calloc test_calloc
#define mmap test_mmap
#include "../crates/dream-mir/src/runtime/c/native/heap.c"
#undef calloc
#undef mmap

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
    alarm(5);
    assert(argc == 2);
    if (strcmp(argv[1], "private-size") == 0) { dream_malloc_slow(SIZE_MAX, 0); }
    if (strcmp(argv[1], "shared-size") == 0) { dream_malloc_shared(-1, 0); }
    if (strcmp(argv[1], "realloc-size") == 0) { dream_realloc(0, SIZE_MAX, 0); }
    if (strcmp(argv[1], "array-size") == 0) { dream_array_realloc(0, -1, 8); }
    if (strcmp(argv[1], "string-size") == 0) { dream_string_bytes(-1); }
    if (strcmp(argv[1], "string-byte-count") == 0) {
        int32_t string[] = {INT32_MAX, 0};
        dream_str_byte_size((dream_ptr)(uintptr_t)string);
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
