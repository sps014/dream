#ifndef DREAM_NODE_POOL_H
#define DREAM_NODE_POOL_H
/* Foreign-completion queue nodes, shared by every native scheduler under the heap lock. */
#include "dream_platform_internal.h"

typedef struct Node {
    dream_ptr f;
    struct Node *next;
} Node;

#define NODE_BLOCK_CAPACITY 64
#define NODE_CACHED_BLOCKS 8

typedef struct NodeBlock {
    struct NodeBlock *next;
    unsigned used;
    Node nodes[NODE_BLOCK_CAPACITY];
} NodeBlock;

typedef struct NodePool {
    NodeBlock *blocks;
    Node *free;
    size_t live;
} NodePool;

typedef struct NodeCache {
    NodeBlock *blocks;
    unsigned count;
} NodeCache;

static NodeCache foreign_cache;
static NodePool foreign_pool;

static void node_pool_attach(NodePool *pool, NodeBlock *block) {
    block->next = pool->blocks;
    pool->blocks = block;
    block->used = 0;
}

static Node *node_pool_pop(NodePool *pool) {
    Node *node;
    if (pool->free) {
        node = pool->free;
        pool->free = node->next;
    } else {
        node = &pool->blocks->nodes[pool->blocks->used++];
    }
    pool->live++;
    node->f = 0;
    node->next = NULL;
    return node;
}

static void node_blocks_free(NodeBlock *blocks) {
    while (blocks) {
        NodeBlock *next = blocks->next;
        dream_platform_current->deallocate(blocks);
        blocks = next;
    }
}

/* Cached blocks contain no live queue entries. Their cursor resets on adoption; stale slot
 * links are never read. Initializing slots lazily avoids scanning a block for each short burst. */
static NodeBlock *node_pool_retire(NodePool *pool, NodeCache *cache) {
    NodeBlock *discard = NULL;
    pool->free = NULL;
    while (pool->blocks) {
        NodeBlock *block = pool->blocks;
        pool->blocks = block->next;
        if (cache->count < NODE_CACHED_BLOCKS) {
            block->next = cache->blocks;
            cache->blocks = block;
            cache->count++;
        } else {
            block->next = discard;
            discard = block;
        }
    }
    return discard;
}

static NodeBlock *node_cache_pop(NodeCache *cache) {
    NodeBlock *block = cache->blocks;
    if (block) {
        cache->blocks = block->next;
        cache->count--;
    }
    return block;
}

static NodeBlock *node_block_allocate(dream_counter counter) {
    NodeBlock *block = dream_platform_current->allocate(sizeof(*block));
    if (!block) { DREAM_PANIC_LITERAL(u"panic: scheduler allocation failed"); }
    dream_count(counter, 1);
    return block;
}

static Node *foreign_node_take(void) {
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    if (!foreign_pool.free && (!foreign_pool.blocks || foreign_pool.blocks->used == NODE_BLOCK_CAPACITY)) {
        NodeBlock *block = node_cache_pop(&foreign_cache);
        if (!block) {
            /* Allocation and panic hooks may enter the guest heap. Neither runs under its lock. */
            dream_platform_current->unlock(DREAM_LOCK_HEAP);
            block = node_block_allocate(DREAM_COUNT_FOREIGN_ALLOC);
            dream_platform_current->lock(DREAM_LOCK_HEAP);
        }
        node_pool_attach(&foreign_pool, block);
    }
    Node *node = node_pool_pop(&foreign_pool);
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
    return node;
}

static void foreign_node_put(Node *node) {
    NodeBlock *discard = NULL;
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    node->f = 0;
    node->next = foreign_pool.free;
    foreign_pool.free = node;
    foreign_pool.live--;
    if (!foreign_pool.live) { discard = node_pool_retire(&foreign_pool, &foreign_cache); }
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
    node_blocks_free(discard);
}
#endif
