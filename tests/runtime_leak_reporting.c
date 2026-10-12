#include "dream_core.h"
#include <stdio.h>
#include <stdlib.h>
static int heap_used;
int64_t debug_get_live_objects(void) { if (!heap_used) { abort(); } return 42; }
int64_t debug_get_total_allocations(void) { if (!heap_used) { abort(); } return 99; }
void debug_dump_live(void) { if (!heap_used) { abort(); } fputs("[dream] leak by type: Fixture=42\n", stderr); }
int main(int argc, char **argv) {
    if (argc != 3) { return 2; }
    heap_used = atoi(argv[1]);
    dream_llvm_leak_report(atoi(argv[2]), heap_used);
    return 0;
}
