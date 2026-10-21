#ifndef DREAM_LEAK_SCAN_H
#define DREAM_LEAK_SCAN_H

#include <stdint.h>

#define DREAM_LEAK_STRING_SAMPLES 20
#define DREAM_LEAK_STRING_UNITS 40

typedef struct {
    int32_t units;
    uint16_t text[DREAM_LEAK_STRING_UNITS];
} dream_leak_string;

struct dream_leak_type;

/* Live blocks found by one walk of the native heap. Counts are 64-bit so heaps past
 * 2^31 minimum-size blocks (64 GiB) still tally exactly. */
typedef struct {
    struct dream_leak_type *types;
    uint64_t objects;
    uint64_t bytes;
    int32_t string_count;
    dream_leak_string strings[DREAM_LEAK_STRING_SAMPLES];
} dream_leak_scan;

/* Walks every heap mapping under the heap lock. `scan` starts zeroed. */
void dream_leak_scan_heap(dream_leak_scan *scan);
/* Adds `count` live blocks of `tag` spanning `bytes` in total. */
void dream_leak_tally(dream_leak_scan *scan, int32_t tag, uint64_t count, uint64_t bytes);
/* Prints the per-type and string-sample lines, then releases the scan's type table. */
void dream_leak_print(dream_leak_scan *scan);

#endif
