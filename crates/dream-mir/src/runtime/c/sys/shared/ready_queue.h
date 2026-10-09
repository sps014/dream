#ifndef DREAM_READY_QUEUE_H
#define DREAM_READY_QUEUE_H

typedef struct ReadyNode {
    dream_ptr f;
    struct ReadyNode *next;
    int pooled;
} ReadyNode;

/* Ready entries never cross threads or hold a timer deadline. Bounded TLS storage avoids
 * allocator traffic on each poll; larger bursts retain the ordinary allocation path. */
#define READY_NODE_CAPACITY 64
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
        node->pooled = 1;
    } else {
        node = (ReadyNode *)malloc(sizeof(ReadyNode));
        if (!node) { DREAM_PANIC_LITERAL(u"panic: scheduler allocation failed"); }
        dream_count(DREAM_COUNT_READY_ALLOC, 1);
        node->pooled = 0;
    }
    node->f = f;
    node->next = NULL;
    return node;
}

static void ready_put(ReadyNode *node) {
    if (node->pooled) {
        node->f = 0;
        node->next = ready_free;
        ready_free = node;
    } else {
        free(node);
    }
}

#endif
