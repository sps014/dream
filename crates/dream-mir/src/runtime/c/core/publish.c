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

typedef struct PublishBlock {
    struct PublishBlock *next;
    size_t used;
    size_t capacity;
    PublishEntry entries[];
} PublishBlock;

typedef struct {
    PublishEntry *seen;
    PublishEntry *pending;
    PublishBlock *blocks;
} PublishGraph;

#define PUBLISH_CACHED_BLOCKS 16
/* Shared wasm instances replay data segments. Native cache ownership cannot live in those
 * globals; wasm scratch instead returns directly to its guest allocator after each call. */
#ifndef DREAM_WASM32
static PublishBlock *cached_blocks;
static size_t cached_block_count;
#endif

static PublishBlock *publish_block_take(size_t capacity) {
    PublishBlock *block = NULL;
#ifndef DREAM_WASM32
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    PublishBlock **link = &cached_blocks;
    while (*link && (*link)->capacity < capacity) { link = &(*link)->next; }
    block = *link;
    if (block) {
        *link = block->next;
        cached_block_count--;
    }
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
#endif
    if (!block) {
        block = dream_platform_current->allocate(sizeof(*block) + capacity * sizeof(PublishEntry));
        if (!block) { publish_oom(); }
        block->capacity = capacity;
    }
    block->used = 0;
    return block;
}

static void publish_block_put(PublishBlock *block) {
    /* Process ownership avoids scratch storage stranded in retired threads. Active traversals
     * remove their blocks from the cache, so callbacks can safely publish recursively. */
#ifndef DREAM_WASM32
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    if (cached_block_count < PUBLISH_CACHED_BLOCKS) {
        block->next = cached_blocks;
        cached_blocks = block;
        cached_block_count++;
        block = NULL;
    }
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
#endif
    if (block) { dream_platform_current->deallocate(block); }
}

static PublishEntry *publish_entry(PublishGraph *graph) {
    PublishBlock *block = graph->blocks;
    if (block == NULL || block->used == block->capacity) {
        size_t capacity = block == NULL ? 16 : block->capacity * 2;
        if (capacity > 256) { capacity = 256; }
        block = publish_block_take(capacity);
        block->next = graph->blocks;
        graph->blocks = block;
    }
    return &block->entries[block->used++];
}

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
    entry = publish_entry(graph);
    entry->ptr = ptr;
    HASH_ADD(hh, graph->seen, ptr, sizeof(ptr), entry);
    entry->pending_next = graph->pending;
    graph->pending = entry;
}

static void publish_edge(dream_ptr child, void *context) {
    publish_enqueue(context, child);
}

#ifdef DREAM_WASM32_THREADS
DREAM_WASM_EXPORT("dream_publish")
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
    /* Clear the table before returning entries; another traversal may immediately reuse them. */
    HASH_CLEAR(hh, graph.seen);
    while (graph.blocks != NULL) {
        PublishBlock *block = graph.blocks;
        graph.blocks = block->next;
        publish_block_put(block);
    }
}
