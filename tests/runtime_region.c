#include "../crates/dream-mir/src/runtime/c/native/include/dream_region.h"
#include "../crates/dream-mir/src/runtime/c/native/include/dream_thread.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

void dream_future_fini(dream_ptr pointer) { (void)pointer; }
void dream_panic(dream_ptr message) {
    (void)message;
    fputs("region runtime panic\n", stderr);
    abort();
}

static void exercise(void) {
    dream_ptr pointers[20];
    for (int depth = 0; depth < 20; ++depth) {
        dream_region_enter();
        pointers[depth] = dream_malloc(128, TAG_STRUCT_BASE);
        *(int32_t *)dream_p(pointers[depth]) = depth;
        assert(dream_region_owns(pointers[depth]) == (depth < 8));
    }
    for (int depth = 19; depth >= 0; --depth) {
        assert(*(int32_t *)dream_p(pointers[depth]) == depth);
        dream_recycle(pointers[depth]);
        dream_region_leave();
    }
    assert(!dream_region_owns(pointers[0]));

    dream_region_enter();
    dream_ptr outer = dream_malloc(128, TAG_STRUCT_BASE);
    *(int32_t *)dream_p(outer) = 42;
    dream_region_enter();
    for (int index = 0; index < 300000; ++index) {
        dream_ptr pointer = dream_malloc(128, TAG_STRUCT_BASE);
        assert(dream_region_owns(pointer));
        *(int32_t *)dream_p(pointer) = index;
        dream_recycle(pointer);
    }
    dream_ptr large = dream_malloc(9 << 20, TAG_STRUCT_BASE);
    assert(dream_region_owns(large));
    ((char *)dream_p(large))[(9 << 20) - 1] = 7;
    dream_recycle(large);
    dream_region_leave();
    assert(*(int32_t *)dream_p(outer) == 42);
    assert(dream_region_owns(outer));
    dream_ptr reused = dream_malloc(128, TAG_STRUCT_BASE);
    assert(reused != outer);
    dream_recycle(reused);
    dream_recycle(outer);
    dream_region_leave();
}

static DREAM_THREAD_PROC(worker) {
    (void)arg;
    exercise();
    return 0;
}

int main(void) {
    dream_thread thread;
    assert(dream_thread_start(&thread, worker, NULL) == 0);
    exercise();
    dream_thread_join(thread);
    assert(debug_get_live_objects() == 0);
    puts("region stress passed");
    return 0;
}
