#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_heap_maps.h"
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
static int allocations, resizes, mappings, releases, frees, locked[3];
static dream_mutex locks[3] = {DREAM_MUTEX_INIT, DREAM_MUTEX_INIT, DREAM_MUTEX_INIT};

static DREAM_THREAD_PROC(increment_counter) {
    for (int i = 0; i < 100000; ++i) {
        dream_ptr object = dream_malloc(16, 1);
        dream_recycle(object);
    }
    __atomic_fetch_add((int *)arg, 1, __ATOMIC_RELEASE);
    dream_heap_thread_exit();
    return 0;
}
static void *allocate(size_t size) { __atomic_fetch_add(&allocations, 1, __ATOMIC_RELAXED); return fail_allocate ? NULL : malloc(size); }
static void *resize(void *ptr, size_t size) { __atomic_fetch_add(&resizes, 1, __ATOMIC_RELAXED); return fail_resize ? NULL : realloc(ptr, size); }
static void deallocate(void *ptr) { __atomic_fetch_add(&frees, 1, __ATOMIC_RELAXED); free(ptr); }
static void *map(size_t size) { __atomic_fetch_add(&mappings, 1, __ATOMIC_RELAXED); return fail_map ? NULL : calloc(1, size); }
/* Scribbles over released pages, so a block whose contents the allocator still trusted would fail. */
static size_t release(void *ptr, size_t size) {
    __atomic_fetch_add(&releases, 1, __ATOMIC_RELAXED);
    uintptr_t start = ((uintptr_t)ptr + 4095u) & ~(uintptr_t)4095u;
    uintptr_t end = ((uintptr_t)ptr + size) & ~(uintptr_t)4095u;
    if (end <= start) { return 0; }
    memset((void *)start, 0xa5, end - start);
    return end - start;
}
static int64_t clock_ns = 10000000000;
static int64_t now(void) { return clock_ns; }
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
static DREAM_THREAD_PROC(retire_heap) {
    dream_ptr *result = arg;
    dream_ptr live = dream_malloc_shared(16, 1);
    dream_i32(live)[0] = 42;
    for (int i = 0; i < 1000; i++) {
        dream_ptr temporary = dream_malloc(16, 1);
        dream_recycle(temporary);
    }
    *result = live;
    dream_heap_thread_exit();
    dream_heap_thread_exit();
    return 0;
}
static void panic_hook(const char *text, const char *location) {
    assert(strcmp(text, "unicode: \xF0\x9F\x98\x80") == 0);
    assert(strcmp(location, "host/library.dream:17") == 0);
    fputs("HOOK\n", stderr);
}
static void check_utf8_roundtrip(void) {
    const char text[] = "A\xC3\xA9\xE6\x97\xA5\xF0\x9F\x98\x80\xF0\x90\x80\x80\xF4\x8F\xBF\xBF!";
    const uint16_t expected[] = u"A\u00E9\u65E5\U0001F600\U00010000\U0010FFFF!";
    int32_t count = (int32_t)(sizeof(expected) / sizeof(expected[0])) - 1;
    dream_ptr decoded = dream_utf8_to_string(text);
    assert(dream_str_len(decoded) == count);
    assert(memcmp(dream_str_units(decoded), expected, (size_t)count * 2) == 0);
    dream_ptr bytes = dream_array_new(sizeof(text) - 1, 1);
    memcpy((char *)dream_p(bytes) + 4, text, sizeof(text) - 1);
    dream_ptr full = string_from_utf8(bytes);
    dream_ptr prefix = string_from_utf8_prefix(bytes, sizeof(text) - 2);
    assert(dream_str_len(full) == count);
    assert(memcmp(dream_str_units(full), expected, (size_t)count * 2) == 0);
    assert(dream_str_len(prefix) == count - 1);
    assert(memcmp(dream_str_units(prefix), expected, (size_t)(count - 1) * 2) == 0);
    dream_ptr original = dream_string_alloc(count);
    memcpy((char *)dream_p(original) + STRING_UNITS_OFFSET, expected, (size_t)count * 2);
    char *encoded = dream_string_to_utf8(original);
    assert(encoded != NULL && strcmp(encoded, text) == 0);
    dream_platform_current->deallocate(encoded);
    dream_release(original);
    dream_release(prefix);
    dream_release(full);
    dream_release(bytes);
    dream_release(decoded);
}
static void check_invalid_utf8(void) {
    const struct { const char *bytes; const uint16_t *expected; } cases[] = {
        {"\xF0\x9F\x98", u"\uFFFD"},
        {"\xF0\x9F!", u"\uFFFD!"},
        {"\xF0\x80\x80\x80", u"\uFFFD\uFFFD\uFFFD\uFFFD"},
        {"\xF4\x90\x80\x80", u"\uFFFD\uFFFD\uFFFD\uFFFD"},
        {"\xED\xA0\x80", u"\uFFFD\uFFFD\uFFFD"},
        {"\xC0\x80", u"\uFFFD\uFFFD"},
        {"\xE2\x82!", u"\uFFFD!"},
        {"\x80", u"\uFFFD"},
    };
    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        dream_ptr text = dream_utf8_to_string(cases[i].bytes);
        int32_t count = 0;
        while (cases[i].expected[count]) { count++; }
        assert(dream_str_len(text) == count);
        assert(memcmp(dream_str_units(text), cases[i].expected, (size_t)count * 2) == 0);
        dream_release(text);
    }
    const uint16_t units[] = {0xd800, 'a', 0xdc00, 0, 0xd83d, 0xde00};
    const char expected[] = "\xEF\xBF\xBD" "a\xEF\xBF\xBD\0\xF0\x9F\x98\x80";
    dream_ptr text = dream_string_alloc(6);
    memcpy((char *)dream_p(text) + STRING_UNITS_OFFSET, units, sizeof(units));
    char *encoded = dream_string_to_utf8(text);
    assert(encoded != NULL && memcmp(encoded, expected, sizeof(expected)) == 0);
    dream_platform_current->deallocate(encoded);
    dream_release(text);
}
int main(int argc, char **argv) {
    assert(argc == 2);
    dream_platform platform = {allocate, resize, deallocate, map, release, now, terminal_abort, write_bytes,
                               lock, unlock, object_drop};
    dream_set_platform(&platform);
    if (strcmp(argv[1], "raw-memory") == 0) {
        int before_allocations = allocations, before_frees = frees;
        uintptr_t p = dream_raw_alloc(16, 128, _Alignof(int32_t), 0);
        assert(p % 128 == 0);
        ((int32_t *)p)[0] = 42;
        uintptr_t next = dream_raw_realloc(p, 32, _Alignof(int32_t));
        assert(next % 128 == 0 && ((int32_t *)next)[0] == 42);
        dream_raw_free(next);
        assert(allocations - before_allocations == frees - before_frees);
        uintptr_t arena = dream_arena_new(16);
        uintptr_t first = dream_arena_alloc(arena, 8, _Alignof(int64_t), 0);
        ((int64_t *)first)[0] = 99;
        uintptr_t large = dream_arena_alloc(arena, 128, 64, 1);
        assert(large % 64 == 0 && ((int64_t *)large)[0] == 0);
        assert(((int64_t *)first)[0] == 99);
        int grown_allocations = allocations;
        dream_arena_reset(arena);
        assert(dream_arena_alloc(arena, 8, _Alignof(int64_t), 0) == first);
        assert(dream_arena_alloc(arena, 128, 64, 0) == large);
        assert(allocations == grown_allocations);
        uintptr_t largest = dream_arena_alloc(arena, 1024, 256, 0);
        assert(largest % 256 == 0);
        grown_allocations = allocations;
        for (int pass = 0; pass < 4; pass++) {
            dream_arena_reset(arena);
            assert(dream_arena_alloc(arena, 0, 8, 0) == 0);
            uintptr_t reused = dream_arena_alloc(arena, 8, _Alignof(int64_t), 0);
            assert(reused == first);
            ((int64_t *)reused)[0] = 99 + pass;
            if (pass & 1) {
                assert(dream_arena_alloc(arena, 1024, 256, 1) == largest);
                for (int index = 0; index < 1024; index++) { assert(((unsigned char *)largest)[index] == 0); }
                memset((void *)largest, 0xff, 1024);
            } else {
                assert(dream_arena_alloc(arena, 128, 64, 0) == large);
                memset((void *)large, 0xff, 128);
            }
            assert(((int64_t *)reused)[0] == 99 + pass);
            assert(allocations == grown_allocations);
        }
        dream_arena_free(arena);
        assert(allocations - before_allocations == frees - before_frees);
        return 0;
    }
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
    if (strcmp(argv[1], "thread-retention") == 0) {
        int before_maps = mappings;
        int before_records = allocations;
        int64_t before_live = debug_get_live_objects();
        for (int i = 0; i < 64; i++) {
            dream_ptr result = 0;
            dream_thread thread;
            assert(dream_thread_start(&thread, retire_heap, &result) == 0);
            dream_thread_join(thread);
            assert(dream_heap_is_live(result));
            assert(dream_i32(result)[0] == 42);
            dream_release(result);
            assert(debug_get_live_objects() == before_live);
        }
        assert(mappings - before_maps <= 2);
        assert(allocations - before_records <= 2);
    }
    if (strcmp(argv[1], "idle-release") == 0) {
        enum { BLOCKS = 25, BYTES = 1 << 20 };
        dream_ptr blocks[BLOCKS];
        for (int round = 0; round < 2; round++) {
            for (int i = 0; i < BLOCKS; i++) {
                blocks[i] = dream_malloc(BYTES, 1);
                memset(dream_p(blocks[i]), round + 1, BYTES);
            }
            int mapped = mappings;
            int released = releases;
            assert(dream_heap_released_bytes() == 0);
            for (int i = 0; i < BLOCKS - 1; i++) {
                assert(((unsigned char *)dream_p(blocks[i]))[BYTES - 1] == round + 1);
                dream_recycle(blocks[i]);
            }
            /* Freshly freed blocks stay resident however far they exceed the budget. */
            assert(releases == released);
            clock_ns += 2000000000;
            dream_recycle(blocks[BLOCKS - 1]);
            /* Once they sat idle, all but 15 MiB of them goes back to the OS; the block freed
             * just now stays resident. */
            assert(releases == released + 10);
            assert(dream_heap_released_bytes() >= (uint64_t)10 * (BYTES - 8192));
            assert(mappings == mapped);
        }
        return 0;
    }
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
    check_utf8_roundtrip();
    check_invalid_utf8();
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
