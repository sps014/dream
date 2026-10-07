#include "../crates/dream-mir/src/runtime/c/core/include/dream_core.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include "../crates/dream-mir/src/runtime/c/core/heap.c"

/* Verify the full byte count handed to initialization without committing gigabytes of RAM. */
static size_t zeroed_bytes;
static void *sparse_memset(void *ptr, int value, size_t size) {
    zeroed_bytes = size;
    return size > INT32_MAX ? ptr : memset(ptr, value, size);
}
#undef memset
#define memset sparse_memset
#include "../crates/dream-mir/src/runtime/c/core/strings.c"
#include "../crates/dream-mir/src/runtime/c/sys/shared/async.c"
#undef memset

void *dream_ft_get(int32_t index) { (void)index; return NULL; }
void dream_callback_drain(void) {}
int dream_callback_pending(void) { return 0; }
void dream_callback_set_waker(void (*wake)(void *), void *context) {
    (void)wake;
    (void)context;
}
int64_t timeNowNanos(void) { return 0; }
void dream_panic(dream_ptr message) {
    (void)message;
    fputs("unexpected allocation panic\n", stderr);
    exit(1);
}

static void check_block(dream_ptr ptr, size_t payload) {
    assert(ptr && (uintptr_t)ptr % 16 == 0);
    assert(dream_heap_is_live(ptr));
    assert(*dream_block_size((char *)dream_p(ptr) - DREAM_BLOCK_HEADER) >=
           payload + DREAM_BLOCK_HEADER);
    char *bytes = dream_p(ptr);
    bytes[8] = 17;
    bytes[payload - 1] = 29;
    assert(bytes[8] == 17 && bytes[payload - 1] == 29);
}

int main(int argc, char **argv) {
    alarm(20);
    assert(argc == 2 && sizeof(dream_size) == sizeof(size_t));
    size_t bytes = (size_t)400000000 * 8;
    dream_ptr ptr;
    if (strcmp(argv[1], "private") == 0) {
        ptr = dream_malloc(bytes, TAG_ARRAY);
        check_block(ptr, bytes);
        dream_free(ptr);
        assert(!dream_heap_is_live(ptr));
    } else if (strcmp(argv[1], "shared") == 0) {
        ptr = dream_malloc_shared(bytes, TAG_ARRAY);
        check_block(ptr, bytes);
        dream_free(ptr);
        dream_ptr reused = dream_malloc_shared(bytes, TAG_ARRAY);
        assert(reused == ptr);
        check_block(reused, bytes);
        dream_free(reused);
    } else if (strcmp(argv[1], "realloc") == 0) {
        ptr = dream_malloc(16, TAG_ARRAY);
        *(char *)dream_p(ptr) = 42;
        ptr = dream_realloc(ptr, bytes, TAG_ARRAY);
        assert(*(char *)dream_p(ptr) == 42);
        check_block(ptr, bytes);
        assert(dream_realloc(ptr, 16, TAG_ARRAY) == ptr);
        dream_free(ptr);
    } else if (strcmp(argv[1], "array") == 0) {
        ptr = dream_array_new(400000000, 8);
        assert(zeroed_bytes == bytes + 4 && dream_i32(ptr)[0] == 400000000);
        check_block(ptr, bytes + 4);
        dream_free(ptr);
    } else if (strcmp(argv[1], "from-bytes") == 0) {
        ptr = dream_from_bytes(0, bytes, TAG_ARRAY);
        assert(zeroed_bytes == bytes);
        check_block(ptr, bytes);
        dream_free(ptr);
    } else if (strcmp(argv[1], "future") == 0) {
        ptr = dream_new_future(bytes, 0, 0);
        assert(zeroed_bytes == bytes);
        check_block(ptr, bytes);
        dream_free(ptr);
    } else if (strcmp(argv[1], "string") == 0) {
        ptr = dream_string_alloc(INT32_MAX);
        assert(dream_str_len(ptr) == INT32_MAX && dream_str_unit_cap(ptr) == INT32_MAX);
        check_block(ptr, (size_t)INT32_MAX * 2 + STRING_HEADER_SIZE);
        dream_free(ptr);
    } else if (strcmp(argv[1], "region") == 0) {
        dream_region_enter();
        ptr = dream_malloc(bytes, TAG_ARRAY);
        check_block(ptr, bytes);
        assert(dream_region_owns(ptr));
        dream_region_leave();
        assert(debug_get_live_objects() == 0);
    } else {
        return 1;
    }
    return 0;
}

void dream_release_object(dream_ptr ptr) { dream_release(ptr); }
