#ifndef DREAM_READY_QUEUE_H
#define DREAM_READY_QUEUE_H
#include "dream_platform_internal.h"

typedef struct ReadyNode {
    dream_ptr f;
    struct ReadyNode *next;
} ReadyNode;

/* Ready entries stay on their scheduler. Process-owned spare blocks bound retention across
 * sessions and thread exits, while active bursts allocate once per block rather than per entry. */
#define READY_NODE_CAPACITY 64
#define READY_CACHED_BLOCKS 8
typedef struct ReadyBlock {
    struct ReadyBlock *next;
    ReadyNode nodes[READY_NODE_CAPACITY];
} ReadyBlock;
/* Shared wasm instantiation replays data segments, so spare blocks return to the guest heap
 * instead of entering a process cache in mutable module globals. */
#ifndef DREAM_WASM32
static ReadyBlock *ready_cached;
static unsigned ready_cached_count;
#endif
static _Thread_local ReadyBlock *ready_blocks;
static _Thread_local ReadyNode ready_nodes[READY_NODE_CAPACITY];
static _Thread_local int ready_nodes_used;
static _Thread_local ReadyNode *ready_free;
static _Thread_local ReadyNode *rq_head;
static _Thread_local ReadyNode *rq_tail;

static ReadyNode *ready_take(dream_ptr f) {
    ReadyNode *node;
    if (ready_free) {
        node = ready_free;
        ready_free = node->next;
    } else if (ready_nodes_used < READY_NODE_CAPACITY) {
        node = &ready_nodes[ready_nodes_used++];
    } else {
        ReadyBlock *block = NULL;
#ifndef DREAM_WASM32
        dream_platform_current->lock(DREAM_LOCK_HEAP);
        block = ready_cached;
        if (block) {
            ready_cached = block->next;
            ready_cached_count--;
        }
        dream_platform_current->unlock(DREAM_LOCK_HEAP);
#endif
        if (!block) {
            block = dream_platform_current->allocate(sizeof(*block));
            if (!block) { DREAM_PANIC_LITERAL(u"panic: scheduler allocation failed"); }
            dream_count(DREAM_COUNT_READY_ALLOC, 1);
        }
        block->next = ready_blocks;
        ready_blocks = block;
        for (unsigned i = 1; i < READY_NODE_CAPACITY; i++) {
            block->nodes[i].next = ready_free;
            ready_free = &block->nodes[i];
        }
        node = &block->nodes[0];
    }
    node->f = f;
    node->next = NULL;
    return node;
}

static void ready_put(ReadyNode *node) {
    node->f = 0;
    node->next = ready_free;
    ready_free = node;
}

static void ready_finish(void) {
    /* No queue entries survive a drained run loop. Clear TLS aliases before another scheduler
     * can acquire these blocks from the process cache. */
    ready_free = NULL;
    ready_nodes_used = 0;
    while (ready_blocks) {
        ReadyBlock *block = ready_blocks;
        ready_blocks = block->next;
#ifndef DREAM_WASM32
        dream_platform_current->lock(DREAM_LOCK_HEAP);
        if (ready_cached_count < READY_CACHED_BLOCKS) {
            block->next = ready_cached;
            ready_cached = block;
            ready_cached_count++;
            block = NULL;
        }
        dream_platform_current->unlock(DREAM_LOCK_HEAP);
#endif
        if (block) { dream_platform_current->deallocate(block); }
    }
}

#endif
