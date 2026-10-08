#include "dream_core.h"
#include "dream_platform_internal.h"

#include <string.h>

static _Noreturn void publish_oom(void) {
    DREAM_PANIC_LITERAL(u"out of memory while publishing a Task graph");
}

#define uthash_malloc(size) dream_platform_current->allocate(size)
#define uthash_free(ptr, size) dream_platform_current->deallocate(ptr)
#define uthash_fatal(msg) publish_oom()
#include "uthash.h"

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
    if (__atomic_load_n(dream_rc_word(ptr), __ATOMIC_RELAXED) == 0) {
        DREAM_PANIC_LITERAL(u"panic: publication of a dying object");
    }
    HASH_FIND(hh, graph->seen, &ptr, sizeof(ptr), entry);
    if (entry != NULL) {
        return;
    }
    entry = (PublishEntry *)dream_platform_current->allocate(sizeof(*entry));
    if (entry == NULL) {
        publish_oom();
    }
    entry->ptr = ptr;
    HASH_ADD(hh, graph->seen, ptr, sizeof(ptr), entry);
    entry->pending_next = graph->pending;
    graph->pending = entry;
}

static void publish_edge(dream_ptr child, void *context) {
    publish_enqueue(context, child);
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
        dream_visit_owned(entry->ptr, publish_edge, &graph);
    }
    PublishEntry *entry;
    PublishEntry *next;
    HASH_ITER(hh, graph.seen, entry, next) {
        HASH_DEL(graph.seen, entry);
        dream_platform_current->deallocate(entry);
    }
}
