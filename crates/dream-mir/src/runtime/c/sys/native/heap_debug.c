#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_heap_maps.h"
#include "dream_leak_scan.h"
#include <inttypes.h>
#include <stdio.h>

static _Noreturn void leak_scan_oom(void) {
    DREAM_PANIC_LITERAL(u"out of memory tallying the leak report");
}

/* The table lives outside the Dream heap: the walk holds the heap lock. */
#define uthash_malloc(size) dream_platform_current->allocate(size)
#define uthash_free(ptr, size) dream_platform_current->deallocate(ptr)
#define uthash_fatal(msg) leak_scan_oom()
#include "uthash.h"

#define LEAK_PRINTED_TYPES 16

struct dream_leak_type {
    int32_t tag;
    uint64_t count;
    uint64_t bytes;
    UT_hash_handle hh;
};

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

void dream_leak_tally(dream_leak_scan *scan, int32_t tag, uint64_t count, uint64_t bytes) {
    struct dream_leak_type *entry;
    HASH_FIND_INT(scan->types, &tag, entry);
    if (entry == NULL) {
        entry = dream_platform_current->allocate(sizeof(*entry));
        if (entry == NULL) {
            leak_scan_oom();
        }
        entry->tag = tag;
        entry->count = 0;
        entry->bytes = 0;
        HASH_ADD_INT(scan->types, tag, entry);
    }
    entry->count += count;
    entry->bytes += bytes;
    scan->objects += count;
    scan->bytes += bytes;
}

static void save_string(dream_leak_scan *scan, char *data) {
    if (scan->string_count >= DREAM_LEAK_STRING_SAMPLES) {
        return;
    }
    int32_t units = ((int32_t *)data)[0];
    if (units < 0) {
        units = 0;
    }
    if (units > DREAM_LEAK_STRING_UNITS) {
        units = DREAM_LEAK_STRING_UNITS;
    }
    dream_leak_string *sample = &scan->strings[scan->string_count++];
    sample->units = units;
    for (int32_t i = 0; i < units; i++) {
        sample->text[i] = ((uint16_t *)(data + 8))[i];
    }
}

static void scan_map(dream_leak_scan *scan, char *base, size_t len) {
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
            dream_leak_tally(scan, tag, 1, (uint64_t)sz);
            if (tag == TAG_STRING) {
                save_string(scan, block + NATIVE_HEAP_HEADER_SIZE);
            }
        }
        p += sz;
    }
}

void dream_leak_scan_heap(dream_leak_scan *scan) {
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    for (size_t map = 0; map < dream_heap_map_count_locked(); ++map) {
        size_t size;
        char *base = (char *)dream_heap_map_at_locked(map, &size);
        scan_map(scan, base, size);
    }
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
}

/* Largest footprint first; the tag breaks ties so the report is deterministic. */
static int by_bytes(struct dream_leak_type *a, struct dream_leak_type *b) {
    if (a->bytes != b->bytes) {
        return a->bytes > b->bytes ? -1 : 1;
    }
    return (a->tag > b->tag) - (a->tag < b->tag);
}

void dream_leak_print(dream_leak_scan *scan) {
    struct dream_leak_type *entry;
    struct dream_leak_type *next;
    HASH_SORT(scan->types, by_bytes);
    fputs("[dream] leak by type:", stderr);
    if (scan->types == NULL) {
        fputs(" (none)", stderr);
    }
    unsigned printed = 0;
    uint64_t other_types = 0;
    uint64_t other_objects = 0;
    uint64_t other_bytes = 0;
    HASH_ITER(hh, scan->types, entry, next) {
        if (printed < LEAK_PRINTED_TYPES) {
            fprintf(stderr, " %s=%" PRIu64 " (%" PRIu64 " B)", dream_tag_name(entry->tag), entry->count,
                    entry->bytes);
            printed++;
        } else {
            other_types++;
            other_objects += entry->count;
            other_bytes += entry->bytes;
        }
        HASH_DEL(scan->types, entry);
        dream_platform_current->deallocate(entry);
    }
    if (other_types > 0) {
        fprintf(stderr, " other=%" PRIu64 " types (%" PRIu64 " objects, %" PRIu64 " B)", other_types,
                other_objects, other_bytes);
    }
    fputc('\n', stderr);
    if (scan->string_count > 0) {
        fputs("[dream] leak strings:\n", stderr);
        for (int32_t i = 0; i < scan->string_count; i++) {
            fputs("  \"", stderr);
            for (int32_t j = 0; j < scan->strings[i].units; j++) {
                unsigned u = scan->strings[i].text[j];
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

void debug_dump_live(void) {
    dream_leak_scan scan = {0};
    dream_leak_scan_heap(&scan);
    dream_leak_print(&scan);
}
