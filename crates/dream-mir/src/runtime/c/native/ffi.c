#include "include/dream_rt_native.h"

#include <stdio.h>
#include <stdlib.h>

int64_t dream_ffi_read_ptr(int64_t base, int32_t index) {
    if (!base) {
        return 0;
    }
    return (int64_t)(uintptr_t)((void **)(uintptr_t)base)[index];
}

int32_t dream_ffi_read_i32(int64_t base, int32_t index) {
    return ((const int32_t *)(uintptr_t)base)[index];
}

int64_t dream_ffi_read_i64(int64_t base, int32_t index) {
    return ((const int64_t *)(uintptr_t)base)[index];
}

double dream_ffi_read_f64(int64_t base, int32_t index) {
    return ((const double *)(uintptr_t)base)[index];
}

dream_ptr dream_ffi_read_cstring(int64_t ptr) {
    if (!ptr) {
        return dream_string_alloc(0);
    }
    return dream_utf8_to_string((const char *)(uintptr_t)ptr);
}

/* Set on threads Dream started (the process main thread and `Task` workers). Dream's ARC and
 * scheduler state are per-thread, so a callback arriving on a thread C created cannot run. */
_Thread_local int32_t dream_thread_attached;

void dream_thread_attach(void) {
    dream_thread_attached = 1;
}

void dream_callback_enter(void) {
    if (DREAM_UNLIKELY(!dream_thread_attached)) {
        fputs("dream: a C callback ran on a thread Dream did not start; call back from the "
              "thread that registered it\n",
              stderr);
        abort();
    }
}

/* Called by generated C++ shims: a `std::function` adapter holds its `NativeCallback` across the
 * call that handed it over, and drops it when the last copy of the `std::function` dies. */
void dream_callback_retain(dream_ptr obj) {
    dream_callback_enter();
    dream_retain(obj);
}

void dream_callback_release(dream_ptr obj) {
    dream_callback_enter();
    dream_release_object(obj);
}
