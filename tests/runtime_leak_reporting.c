#include "dream_core.h"
#include "dream_leak_scan.h"
#include <stdio.h>
#include <stdlib.h>
static int heap_used;
int64_t debug_get_live_objects(void) { if (!heap_used) { abort(); } return 42; }
int64_t debug_get_total_allocations(void) { if (!heap_used) { abort(); } return 99; }
void dream_leak_scan_heap(dream_leak_scan *scan) { if (!heap_used) { abort(); } scan->bytes = 3000000000u; }
void dream_leak_print(dream_leak_scan *scan) { (void)scan; fputs("[dream] leak by type: Fixture=42 (1344 B)\n", stderr); }
int main(int argc, char **argv) {
    if (argc != 2) { return 2; }
    heap_used = atoi(argv[1]);
    dream_llvm_leak_report(heap_used);
    return 0;
}
