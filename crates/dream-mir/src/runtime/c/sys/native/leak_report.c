#include "dream_core.h"
#ifndef DREAM_WASM32
#include "dream_leak_scan.h"
#include <stdio.h>
#include <inttypes.h>

/* The exit-time heap report native `main` prints in debug builds; release modules never call it.
 * Lives here so generated IR never spells libc's `stderr`. */
void dream_llvm_leak_report(int32_t heap_used) {
    if (!heap_used) {
        fputs("[dream] leak check: live=0 total_allocations=0 live_bytes=0\n"
              "[dream] leak by type: (none)\n",
              stderr);
        return;
    }
    dream_leak_scan scan = {0};
    dream_leak_scan_heap(&scan);
    fprintf(stderr, "[dream] leak check: live=%" PRId64 " total_allocations=%" PRId64 " live_bytes=%" PRIu64 "\n",
            debug_get_live_objects(), debug_get_total_allocations(), scan.bytes);
    dream_leak_print(&scan);
}
#endif
