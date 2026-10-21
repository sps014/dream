#include "dream_core.h"
#include "dream_leak_scan.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

void dream_future_fini(dream_ptr ptr) {
    (void)ptr;
    abort();
}

void dream_panic(dream_ptr message) {
    (void)message;
    fputs("tally runtime panic\n", stderr);
    abort();
}

void dream_release_object(dream_ptr ptr) { dream_release(ptr); }

int main(void) {
    dream_leak_scan scan = {0};
    dream_leak_tally(&scan, 1000, UINT64_C(1500000000), UINT64_C(48000000000));
    dream_leak_tally(&scan, 1000, UINT64_C(1500000000), UINT64_C(48000000000));
    dream_leak_tally(&scan, 1001, UINT64_C(3000000000), UINT64_C(64000000000));
    for (int32_t tag = 100; tag < 400; tag++) {
        dream_leak_tally(&scan, tag, 1, 16);
    }
    assert(scan.objects == UINT64_C(6000000300));
    assert(scan.bytes == UINT64_C(160000004800));
    dream_leak_print(&scan);
    assert(scan.types == NULL);
    puts("tally passed");
    return 0;
}
