#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_thread.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

_Thread_local dream_ptr g0;
void *dream_ft_get(int32_t index) { (void)index; return NULL; }
void dream_future_fini(dream_ptr ptr) { (void)ptr; }
void dream_release_object(dream_ptr ptr) { dream_release(ptr); }
const dream_platform dream_default_platform = {0};
static int fail_allocate, fail_resize, fail_map;
static int allocations, resizes, mappings, frees, locked[3];
static dream_mutex locks[3] = {DREAM_MUTEX_INIT, DREAM_MUTEX_INIT, DREAM_MUTEX_INIT};

static DREAM_THREAD_PROC(increment_counter) {
    for (int i = 0; i < 100000; ++i) {
        dream_ptr object = dream_malloc(16, 1);
        dream_recycle(object);
    }
    __atomic_fetch_add((int *)arg, 1, __ATOMIC_RELEASE);
    return 0;
}
static void *allocate(size_t size) { __atomic_fetch_add(&allocations, 1, __ATOMIC_RELAXED); return fail_allocate ? NULL : malloc(size); }
static void *resize(void *ptr, size_t size) { __atomic_fetch_add(&resizes, 1, __ATOMIC_RELAXED); return fail_resize ? NULL : realloc(ptr, size); }
static void deallocate(void *ptr) { __atomic_fetch_add(&frees, 1, __ATOMIC_RELAXED); free(ptr); }
static void *map(size_t size) { __atomic_fetch_add(&mappings, 1, __ATOMIC_RELAXED); return fail_map ? NULL : calloc(1, size); }
static void write_bytes(int stream, const void *bytes, size_t size, int encoding) {
    assert(stream == 2);
    if (encoding == DREAM_TEXT_UTF16) { dream_write_utf16(stream, bytes, (int32_t)size); return; }
    fwrite(bytes, 1, size, stderr);
}
static void terminal_abort(void) { fputs("PLATFORM_ABORT\n", stderr); fflush(stderr); _Exit(86); }
static void lock(unsigned domain) {
    assert(domain < 3);
    dream_mutex_lock(&locks[domain]);
    assert(!locked[domain]);
    locked[domain] = 1;
}
static void unlock(unsigned domain) {
    assert(domain < 3 && locked[domain]);
    locked[domain] = 0;
    dream_mutex_unlock(&locks[domain]);
}
static void object_drop(void *ptr) { (void)ptr; }
static void panic_hook(const char *text, const char *location) {
    assert(strcmp(text, "unicode: \xF0\x9F\x98\x80") == 0);
    assert(strcmp(location, "host/library.dream:17") == 0);
    fputs("HOOK\n", stderr);
}
int main(int argc, char **argv) {
    assert(argc == 2);
    dream_platform platform = {allocate, resize, deallocate, map, terminal_abort, write_bytes,
                               lock, unlock, object_drop};
    dream_set_platform(&platform);
    if (strcmp(argv[1], "counters") == 0) { fail_allocate = 1; }
    if (strcmp(argv[1], "mapping") == 0) { fail_map = 1; }
    if (strcmp(argv[1], "index") == 0) { fail_resize = 1; }
    const uint16_t units[] = u"unicode: \U0001F600";
    dream_ptr string = dream_string_alloc(11);
    memcpy((char *)dream_p(string) + STRING_UNITS_OFFSET, units, 11 * sizeof(uint16_t));
    if (strcmp(argv[1], "panic") == 0 || strcmp(argv[1], "hook") == 0) {
        if (strcmp(argv[1], "hook") == 0) { dream_set_panic_hook(panic_hook); }
        dream_panic_at(string, "host/library.dream:17");
    }
    if (strcmp(argv[1], "nul") == 0) {
        const uint16_t text[] = {'a', 0, 'b'};
        platform.write(2, text, 3, DREAM_TEXT_UTF16);
        return 0;
    }
    dream_ptr hello = dream_utf8_to_string("hello");
    if (strcmp(argv[1], "wide-counters") == 0) {
        dream_heap_counters *counters = dream_heap.fast;
        assert(counters != NULL);
        int64_t live = debug_get_live_objects();
        int64_t total = debug_get_total_allocations();
        uint64_t offset = UINT64_C(1) << 32;
        dream_heap_count(&counters->allocs, offset);
        dream_heap_count(&counters->frees, offset);
        assert(debug_get_live_objects() == live);
        assert(debug_get_total_allocations() == total + (int64_t)offset);
        dream_thread workers[8];
        int completed = 0;
        for (int i = 0; i < 8; ++i) {
            assert(dream_thread_start(&workers[i], increment_counter, &completed) == 0);
        }
        int64_t previous = debug_get_total_allocations();
        while (__atomic_load_n(&completed, __ATOMIC_ACQUIRE) != 8) {
            int64_t snapshot = debug_get_total_allocations();
            assert(snapshot >= previous);
            previous = snapshot;
            assert(debug_get_live_objects() >= 0);
            dream_thread_yield();
        }
        for (int i = 0; i < 8; ++i) { dream_thread_join(workers[i]); }
        assert(debug_get_total_allocations() == total + (int64_t)offset + 800000);
        assert(debug_get_live_objects() == live);
    }
    if (strcmp(argv[1], "long") == 0) {
        dream_ptr message = dream_string_alloc(3000);
        uint16_t *text = (uint16_t *)((char *)dream_p(message) + STRING_UNITS_OFFSET);
        for (int i = 0; i < 3000; ++i) { text[i] = 'x'; }
        dream_panic_at(message, NULL);
    }
    char *copy = dream_string_to_utf8(hello);
    assert(strcmp(copy, "hello") == 0);
    platform.deallocate(copy);
    dream_release(hello);
    dream_retain(string);
    dream_release(string);
    dream_release(string);
    assert(allocations > 0 && resizes > 0 && mappings > 0 && frees > 0);
    assert(!locked[0] && !locked[1]);
    puts("injected platform passed");
    return 0;
}
