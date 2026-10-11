#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_heap_maps.h"
#include <stdio.h>

__attribute__((weak)) const char *dream_tag_name(int32_t tag) {
    switch (tag & TAG_VALUE_MASK) {
    case 0:
        return "future";
    case TAG_STRING:
        return "string";
    case TAG_ARRAY:
        return "array";
    case TAG_CLOSURE_ENV:
        return "closure_env";
    case TAG_FUNCBOX:
        return "funcbox";
    case TAG_FUTURE:
        return "future";
    default:
        return "object";
    }
}

#define DUMP_HIST 256
#define DUMP_STR_SAMPLES 20
#define DUMP_STR_UNITS 40

typedef struct {
    int32_t tag;
    int32_t n;
} DumpHist;

typedef struct {
    int32_t n;
    uint16_t u[DUMP_STR_UNITS];
} DumpStr;

static void dump_hist_add(DumpHist *h, int *nh, int32_t tag) {
    int i;
    for (i = 0; i < *nh; i++) {
        if (h[i].tag == tag) {
            h[i].n += 1;
            return;
        }
    }
    if (*nh < DUMP_HIST) {
        h[*nh].tag = tag;
        h[*nh].n = 1;
        *nh += 1;
    }
}

static void dump_string_save(char *data, DumpStr *ss, int32_t *printed) {
    int32_t n;
    int32_t i;
    if (*printed >= DUMP_STR_SAMPLES) {
        return;
    }
    n = ((int32_t *)data)[0];
    if (n < 0) {
        n = 0;
    }
    if (n > DUMP_STR_UNITS) {
        n = DUMP_STR_UNITS;
    }
    ss[*printed].n = n;
    for (i = 0; i < n; i++) {
        ss[*printed].u[i] = ((uint16_t *)(data + 8))[i];
    }
    *printed += 1;
}

static void dump_scan_map(char *base, size_t len, DumpHist *h, int *nh, DumpStr *ss,
                          int32_t *str_n) {
    char *p = base;
    char *end = base + len;
    while ((size_t)(end - p) >= NATIVE_HEAP_HEADER_SIZE) {
        unsigned cls = dream_block_class(p);
        char *block = p;
        size_t sz;
        if (cls == DREAM_CLASS_LARGE_PREFIX) {
            block = p + DREAM_LARGE_PREFIX;
            sz = DREAM_LARGE_PREFIX + *dream_large_size(block);
        } else if (cls < DREAM_NCLASS) {
            sz = (size_t)dream_class_bytes((int)cls);
        } else {
            p += 16;
            continue;
        }
        if (sz > (size_t)(end - p)) {
            break;
        }
        // An untouched arena tail reads as zero-count class-0 blocks. Pinned singletons
        // (`dream_pin_immortal`) are never freed by design and already left `live_objects`,
        // so counting them here would report a leak the accounting denies.
        int32_t rc = *(int32_t *)(block + NATIVE_HEAP_HEADER_SIZE - RC_FROM_DATA);
        int32_t raw_tag = *dream_block_tag(block);
        if (raw_tag != DREAM_TAG_FREED && rc != 0 && rc != DREAM_RC_IMMORTAL) {
            int32_t tag = raw_tag & TAG_VALUE_MASK;
            dump_hist_add(h, nh, tag);
            if (tag == TAG_STRING) {
                dump_string_save(block + NATIVE_HEAP_HEADER_SIZE, ss, str_n);
            }
        }
        p += sz;
    }
}

void debug_dump_live(void) {
    DumpHist hist[DUMP_HIST];
    DumpStr samples[DUMP_STR_SAMPLES];
    int nh = 0;
    int32_t str_n = 0;
    int i;
    int j;
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    for (size_t map = 0; map < dream_heap_map_count_locked(); ++map) {
        size_t size;
        char *base = (char *)dream_heap_map_at_locked(map, &size);
        dump_scan_map(base, size, hist, &nh, samples, &str_n);
    }
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
    for (i = 0; i < nh; i++) {
        int best = i;
        for (j = i + 1; j < nh; j++) {
            if (hist[j].n > hist[best].n) {
                best = j;
            }
        }
        if (best != i) {
            DumpHist tmp = hist[i];
            hist[i] = hist[best];
            hist[best] = tmp;
        }
    }
    fputs("[dream] leak by type:", stderr);
    if (nh == 0) {
        fputs(" (none)\n", stderr);
    } else {
        for (i = 0; i < nh && i < 16; i++) {
            fprintf(stderr, " %s=%d", dream_tag_name(hist[i].tag), hist[i].n);
        }
        fputc('\n', stderr);
    }
    if (str_n > 0) {
        fputs("[dream] leak strings:\n", stderr);
        for (i = 0; i < str_n; i++) {
            fputs("  \"", stderr);
            for (j = 0; j < samples[i].n; j++) {
                unsigned u = samples[i].u[j];
                if (u >= 32 && u < 127 && u != '"' && u != '\\') {
                    fputc((int)u, stderr);
                } else {
                    fputc('?', stderr);
                }
            }
            fputs("\"\n", stderr);
        }
    }
}
