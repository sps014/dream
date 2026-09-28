/* External definitions of every `DREAM_ALWAYS_INLINE` helper in `dream_rt_native.h`, for the
 * LLVM backend: it calls these by name, and after `llvm-link` + internalize LLVM inlines them
 * the way clang would inline them from the header. No `used`: that would pin unreferenced helpers
 * through internalize. External definitions are always emitted, so the signature table still
 * sees every helper. */
#define DREAM_ALWAYS_INLINE __attribute__((always_inline))
#include "dream_rt_native.h"

#ifndef DREAM_WASM32
#include <stdio.h>
#include <stdlib.h>

/* The exit-time heap-counter report native `main` prints: always for debug builds, behind
 * `DREAM_DEBUG_LEAKS` otherwise. Lives here so generated IR never spells libc's `stderr`. */
void dream_llvm_leak_report(int32_t always) {
    if (!always && getenv("DREAM_DEBUG_LEAKS") == NULL) {
        return;
    }
    fprintf(stderr, "[dream] leak check: live=%d total_allocations=%d\n", debug_get_live_objects(),
            debug_get_total_allocations());
    debug_dump_live();
}
#endif
