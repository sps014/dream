#include "include/dream_rt_native.h"
#include <stdlib.h>
#include <string.h>

static _Noreturn void publish_oom(void) {
    DREAM_PANIC_LITERAL(u"out of memory while publishing a Task graph");
}

#define uthash_fatal(msg) publish_oom()
#include "include/uthash.h"

typedef struct PublishEntry {
    dream_ptr ptr;
    struct PublishEntry *pending_next;
    UT_hash_handle hh;
} PublishEntry;

typedef struct {
    PublishEntry *seen;
    PublishEntry *pending;
} PublishGraph;

static void publish_enqueue(PublishGraph *graph, dream_ptr ptr) {
    PublishEntry *entry;
    if (!dream_heap_is_live(ptr)) {
        return;
    }
    HASH_FIND(hh, graph->seen, &ptr, sizeof(ptr), entry);
    if (entry != NULL) {
        return;
    }
    entry = (PublishEntry *)malloc(sizeof(*entry));
    if (entry == NULL) {
        publish_oom();
    }
    entry->ptr = ptr;
    HASH_ADD(hh, graph->seen, ptr, sizeof(ptr), entry);
    entry->pending_next = graph->pending;
    graph->pending = entry;
}

static void publish_children(PublishGraph *graph, dream_ptr ptr, int32_t kind) {
    const char *data = (const char *)dream_p(ptr);
    int32_t size;
    memcpy(&size, data - DREAM_BLOCK_HEADER, sizeof(size));
    size_t payload = (size_t)(size - DREAM_BLOCK_HEADER);
    /* Array elements follow a 32-bit length even when native pointers are 64-bit. */
    size_t start = (kind == TAG_ARRAY || kind == TAG_CLOSURE_ENV) ? sizeof(int32_t) : 0;
    for (size_t off = start; off + sizeof(dream_ptr) <= payload; off += sizeof(dream_ptr)) {
        dream_ptr child;
        memcpy(&child, data + off, sizeof(child));
        publish_enqueue(graph, child);
    }
}

#ifdef DREAM_WASM32_THREADS
__attribute__((export_name("dream_publish")))
#endif
void dream_publish(dream_ptr ptr) {
    /* TAG_SHARED is ownership, not visitation: shared roots can contain new private children. */
    PublishGraph graph = {0};
    publish_enqueue(&graph, ptr);
    while (graph.pending != NULL) {
        PublishEntry *entry = graph.pending;
        graph.pending = entry->pending_next;
        int32_t *tag = dream_tag_word(entry->ptr);
        int32_t kind = __atomic_load_n(tag, __ATOMIC_RELAXED) & TAG_VALUE_MASK;
        if (kind == 0) {
            continue;
        }
        __atomic_fetch_or(tag, TAG_SHARED, __ATOMIC_RELAXED);
        int32_t *rc = dream_rc_word(entry->ptr);
        int32_t count = __atomic_load_n(rc, __ATOMIC_RELAXED);
        while (count > 0 && !__atomic_compare_exchange_n(
                   rc, &count, count | DREAM_RC_SHARED_BIT, 0,
                   __ATOMIC_RELAXED, __ATOMIC_RELAXED)) {
        }
        if (kind == TAG_STRING) {
            if (dream_i32(entry->ptr)[1] == DREAM_STR_SLICE) {
                dream_ptr parent;
                memcpy(&parent, (char *)dream_p(entry->ptr) + 8, sizeof(parent));
                publish_enqueue(&graph, parent);
            }
        } else if (kind == TAG_ARRAY || kind >= TAG_STRUCT_BASE) {
            publish_children(&graph, entry->ptr, kind);
        }
    }
    PublishEntry *entry;
    PublishEntry *next;
    HASH_ITER(hh, graph.seen, entry, next) {
        HASH_DEL(graph.seen, entry);
        free(entry);
    }
}
