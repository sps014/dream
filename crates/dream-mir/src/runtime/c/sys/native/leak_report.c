#include "dream_core.h"
#ifndef DREAM_WASM32
#include <stdio.h>
#include <stdlib.h>
#include <inttypes.h>

/* The exit-time heap-counter report native `main` prints: always for debug builds, behind
 * `DREAM_DEBUG_LEAKS` otherwise. Lives here so generated IR never spells libc's `stderr`. */
void dream_llvm_leak_report(int32_t always) {
    dream_defer_drain_all();
    if (!always && getenv("DREAM_DEBUG_LEAKS") == NULL) {
        return;
    }
    fprintf(stderr, "[dream] leak check: live=%" PRId64 " total_allocations=%" PRId64 "\n", debug_get_live_objects(),
            debug_get_total_allocations());
    debug_dump_live();
}
#endif
