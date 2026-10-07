#include "dream_core.h"
#include "dream_platform_internal.h"

#define uthash_malloc(size) dream_platform_current->allocate(size)
#define uthash_free(ptr, size) dream_platform_current->deallocate(ptr)
#define uthash_fatal(msg) DREAM_PANIC_LITERAL(u"panic: out of memory indexing cycle objects")
#include "uthash.h"

typedef struct CycleNode {
    dream_ptr ptr;
    uint64_t sequence;
    unsigned queued;
    int dying;
    UT_hash_handle hh;
} CycleNode;

typedef struct Candidate {
    CycleNode *node;
    dream_ptr ptr;
    void (*destroy)(dream_ptr);
    struct Candidate *next;
} Candidate;

typedef struct {
    unsigned gate_depth;
    int draining;
    int postponed;
    int finalizing;
    int heap_empty;
    Candidate *head;
    Candidate *tail;
    void (*edge)(dream_ptr, void *);
    void *edge_context;
} CycleContext;

#ifndef DREAM_WASM32
static _Thread_local CycleContext thread_context;
static CycleContext *peek_context(void) { return &thread_context; }
static CycleContext *context(void) { return &thread_context; }
#else
extern dream_ptr dream_cycle_context_get(void);
extern void dream_cycle_context_set(dream_ptr);
static CycleContext *peek_context(void) {
    return (CycleContext *)dream_p(dream_cycle_context_get());
}
static CycleContext *context(void) {
    CycleContext *c = peek_context();
    if (!c) {
        c = dream_raw_calloc(1, sizeof(*c));
        if (!c) { DREAM_PANIC_LITERAL(u"panic: out of memory creating cycle context"); }
        dream_cycle_context_set((dream_ptr)c);
    }
    return c;
}
#endif

static void release_idle_context(CycleContext *c) {
#ifdef DREAM_WASM32
    if (c && c->heap_empty && !c->gate_depth && !c->draining && !c->postponed && !c->finalizing && !c->head && !c->edge) {
        dream_cycle_context_set(0);
        dream_platform_current->deallocate(c);
    }
#else
    (void)c;
#endif
}

static CycleNode *nodes;
static uint64_t next_sequence;
static int may_have_forward_edges;

void dream_cycle_enter(void) {
    CycleContext *c = context();
    if (c->gate_depth++ == 0) { dream_platform_current->lock(DREAM_LOCK_CYCLE); }
}
void dream_cycle_leave(void) {
    CycleContext *c = context();
    if (--c->gate_depth == 0) {
        c->heap_empty = nodes == NULL;
        dream_platform_current->unlock(DREAM_LOCK_CYCLE);
    }
    release_idle_context(c);
}

__attribute__((weak)) const dream_type_info *dream_type_info_for_tag(int32_t tag) {
    (void)tag;
    return dream_builtin_type_info(tag);
}

static void string_visit(dream_ptr p) {
    if (dream_i32(p)[1] == DREAM_STR_SLICE) {
        dream_ptr parent;
        memcpy(&parent, (char *)dream_p(p) + 8, sizeof(parent));
        dream_visit_edge(parent);
    }
}
static const dream_type_info string_info = { string_visit, NULL, NULL, NULL, 0 };
static dream_ptr closure_edge(dream_ptr p) {
    dream_ptr child; memcpy(&child, (char *)dream_p(p) + 8, sizeof(child)); return child;
}
static void closure_visit(dream_ptr p) { dream_visit_edge(closure_edge(p)); }
static void closure_clear(dream_ptr p) {
    dream_ptr child = closure_edge(p), zero = 0;
    memcpy((char *)dream_p(p) + 8, &zero, sizeof(zero));
    dream_release_object(child);
}
static void environment_visit(dream_ptr p) {
    int32_t n = dream_i32(p)[0];
    for (int32_t i = 0; i < n; ++i) {
        dream_ptr child;
        memcpy(&child, (char *)dream_p(p) + 4 + (size_t)i * sizeof(child), sizeof(child));
        dream_visit_edge(child);
    }
}
static void environment_clear(dream_ptr p) {
    int32_t n = dream_i32(p)[0];
    for (int32_t i = 0; i < n; ++i) {
        dream_ptr child, zero = 0;
        char *slot = (char *)dream_p(p) + 4 + (size_t)i * sizeof(child);
        memcpy(&child, slot, sizeof(child)); memcpy(slot, &zero, sizeof(zero));
        dream_release_object(child);
    }
}
static const dream_type_info closure_info = { closure_visit, NULL, closure_clear, dream_recycle, 1 };
static const dream_type_info environment_info = { environment_visit, NULL, environment_clear, dream_recycle, 1 };
const dream_type_info *dream_builtin_type_info(int32_t tag) {
    switch (tag) {
    case TAG_STRING: return &string_info;
    case TAG_FUNCBOX: return &closure_info;
    case TAG_CLOSURE_ENV: return &environment_info;
    default: return NULL;
    }
}

static CycleNode *find_node(dream_ptr ptr) {
    CycleNode *node;
    HASH_FIND(hh, nodes, &ptr, sizeof(ptr), node);
    return node;
}

void dream_set_type(dream_ptr ptr, const dream_type_info *info) {
    memcpy((char *)dream_p(ptr) - DREAM_BLOCK_HEADER + sizeof(dream_size), &info, sizeof(info));
    if (!info || !info->cycle_capable || dream_rc_immortal(ptr)) { return; }
    dream_cycle_enter();
    int32_t tag = dream_object_tag(ptr);
    if (tag == TAG_FUTURE || tag == TAG_FUNCBOX || tag == TAG_CLOSURE_ENV || tag == TAG_ARRAY) {
        may_have_forward_edges = 1;
    }
    if (!find_node(ptr)) {
        CycleNode *node = dream_raw_calloc(1, sizeof(*node));
        if (!node || next_sequence == UINT64_MAX) { DREAM_PANIC_LITERAL(u"panic: cycle allocation identity exhausted"); }
        node->ptr = ptr;
        node->sequence = ++next_sequence;
        HASH_ADD(hh, nodes, ptr, sizeof(ptr), node);
    }
    dream_cycle_leave();
}

void dream_visit_edge(dream_ptr ptr) {
    CycleContext *c = peek_context();
    if (ptr && c && c->edge) { c->edge(ptr, c->edge_context); }
}

void dream_visit_owned(dream_ptr ptr, void (*edge)(dream_ptr, void *), void *arg) {
    const dream_type_info *info = dream_object_info(ptr);
    if (!info || !info->visit) { return; }
    CycleContext *c = context();
    void (*saved)(dream_ptr, void *) = c->edge;
    void *saved_context = c->edge_context;
    c->edge = edge;
    c->edge_context = arg;
    info->visit(ptr);
    c->edge = saved;
    c->edge_context = saved_context;
    release_idle_context(c);
}

void dream_cycle_forget(dream_ptr ptr) {
    const dream_type_info *info = dream_object_info(ptr);
    if (!info || !info->cycle_capable) { return; }
    dream_cycle_enter();
    CycleNode *node = find_node(ptr);
    if (node) {
        HASH_DEL(nodes, node);
        if (!nodes) { may_have_forward_edges = 0; }
        node->ptr = 0;
        if (!node->queued) { dream_platform_current->deallocate(node); }
    }
    dream_cycle_leave();
}

static void queue_node(CycleNode *node) {
    Candidate *candidate = dream_raw_calloc(1, sizeof(*candidate));
    if (!candidate) { DREAM_PANIC_LITERAL(u"panic: out of memory queuing cycle candidate"); }
    CycleContext *c = context();
    candidate->node = node;
    ++node->queued;
    if (c->tail) { c->tail->next = candidate; } else { c->head = candidate; }
    c->tail = candidate;
}

int dream_cycle_defer_destroy(dream_ptr ptr, void (*destroy)(dream_ptr)) {
    CycleContext *c = peek_context();
    if (!c || !c->gate_depth) { return 0; }
    Candidate *candidate = dream_raw_calloc(1, sizeof(*candidate));
    if (!candidate) { DREAM_PANIC_LITERAL(u"panic: out of memory queuing destruction"); }
    dream_weak_prepare_destroy(ptr);
    candidate->ptr = ptr;
    candidate->destroy = destroy;
    if (c->tail) { c->tail->next = candidate; } else { c->head = candidate; }
    c->tail = candidate;
    return 1;
}

void dream_cycle_retain(dream_ptr ptr) {
    dream_cycle_enter();
    CycleNode *node = find_node(ptr);
    if (node && node->dying) {
        if (!context()->finalizing) { DREAM_PANIC_LITERAL(u"panic: resurrection of a dying object"); }
    } else {
        int32_t *rc = dream_rc_word(ptr);
        int32_t v = __atomic_load_n(rc, __ATOMIC_RELAXED);
        if (v != DREAM_RC_IMMORTAL) {
            if ((v & INT32_MAX) == INT32_MAX) { DREAM_PANIC_LITERAL(u"panic: reference count overflow"); }
            __atomic_store_n(rc, v + 1, __ATOMIC_RELAXED);
        }
    }
    dream_cycle_leave();
}

void dream_cycle_check_store(dream_ptr owner, dream_ptr child) {
    const dream_type_info *a_info = owner ? dream_object_info(owner) : NULL;
    const dream_type_info *b_info = child ? dream_object_info(child) : NULL;
    if (!(a_info && a_info->cycle_capable) && !(b_info && b_info->cycle_capable)) { return; }
    dream_cycle_enter();
    CycleNode *a = owner ? find_node(owner) : NULL;
    CycleNode *b = child ? find_node(child) : NULL;
    if ((a && a->dying) || (b && b->dying)) {
        DREAM_PANIC_LITERAL(u"panic: publication or mutation of a dying object");
    }
    if (a && b && b->sequence >= a->sequence) { may_have_forward_edges = 1; }
    dream_cycle_leave();
}

int dream_cycle_store_begin(dream_ptr owner, dream_ptr child, int value) {
    const dream_type_info *a = owner ? dream_object_info(owner) : NULL;
    const dream_type_info *b = child ? dream_object_info(child) : NULL;
    int locked = value || (a && a->cycle_capable) || (b && b->cycle_capable);
    if (locked) {
        dream_cycle_enter();
        dream_cycle_check_store(owner, child);
        CycleNode *parent = owner ? find_node(owner) : NULL;
        CycleNode *edge = child ? find_node(child) : NULL;
        if (value == 2 || (parent && edge && edge->sequence >= parent->sequence)) {
            may_have_forward_edges = 1;
        }
    }
    return locked;
}
void dream_cycle_store_end(int locked) {
    if (locked) { dream_cycle_leave(); dream_cycle_drain(); }
}

typedef struct Trial {
    dream_ptr ptr;
    CycleNode *node;
    int64_t residual;
    int live;
    struct Trial *next;
    UT_hash_handle hh;
} Trial;

typedef struct {
    Trial *seen;
    Trial *head;
    Trial *tail;
    int mode;
} TrialGraph;

static void trial_edge(dream_ptr ptr, void *arg) {
    TrialGraph *graph = arg;
    CycleNode *node = find_node(ptr);
    if (!node || node->dying) { return; }
    Trial *entry;
    HASH_FIND(hh, graph->seen, &ptr, sizeof(ptr), entry);
    if (graph->mode == 0 && !entry) {
        entry = dream_raw_calloc(1, sizeof(*entry));
        if (!entry) { DREAM_PANIC_LITERAL(u"panic: out of memory constructing cycle trial"); }
        entry->ptr = ptr;
        entry->node = node;
        entry->residual = dream_rc_count(ptr);
        HASH_ADD(hh, graph->seen, ptr, sizeof(ptr), entry);
        if (graph->tail) { graph->tail->next = entry; } else { graph->head = entry; }
        graph->tail = entry;
    } else if (graph->mode == 1 && entry) {
        --entry->residual;
    } else if (graph->mode == 2 && entry && !entry->live) {
        entry->live = 1;
        entry->next = NULL;
        if (graph->tail) { graph->tail->next = entry; } else { graph->head = entry; }
        graph->tail = entry;
    }
}

static int allocation_order(Trial *x, Trial *y) {
    return x->node->sequence < y->node->sequence ? -1 : x->node->sequence != y->node->sequence;
}

static void collect(CycleNode *candidate) {
    TrialGraph graph = {0};
    if (!candidate->ptr || candidate->dying) { return; }
    // Strictly decreasing allocation identities prove a DAG without walking its descendants.
    // Dynamic shapes and any forward store invalidate this proof until the tracked heap empties.
    if (!may_have_forward_edges && dream_rc_count(candidate->ptr) > 0) { return; }
    // A zero-count object has no incoming strong edge and cannot be part of a cycle.
    // Keep its teardown iterative without allocating trial counts or ordering scratch.
    if (dream_rc_count(candidate->ptr) == 0) {
        dream_ptr ptr = candidate->ptr;
        const dream_type_info *info = dream_object_info(ptr);
        candidate->dying = 1;
        dream_weak_prepare_destroy(ptr);
        dream_cycle_leave();
        CycleContext *c = context();
        ++c->finalizing;
        if (info->finalize) { info->finalize(ptr); }
        --c->finalizing;
        if (info->clear) { info->clear(ptr); }
        info->reclaim(ptr);
        dream_cycle_enter();
        return;
    }
    trial_edge(candidate->ptr, &graph);
    for (Trial *entry = graph.head; entry; entry = entry->next) {
        dream_visit_owned(entry->ptr, trial_edge, &graph);
    }
    graph.mode = 1;
    Trial *entry, *next;
    HASH_ITER(hh, graph.seen, entry, next) {
        dream_visit_owned(entry->ptr, trial_edge, &graph);
    }
    graph.head = graph.tail = NULL;
    graph.mode = 2;
    HASH_ITER(hh, graph.seen, entry, next) {
        if (entry->residual < 0) { DREAM_PANIC_LITERAL(u"panic: invalid strong-edge ownership count"); }
        if (entry->residual > 0) { trial_edge(entry->ptr, &graph); }
    }
    while (graph.head) {
        entry = graph.head;
        graph.head = entry->next;
        if (!graph.head) { graph.tail = NULL; }
        dream_visit_owned(entry->ptr, trial_edge, &graph);
    }
    HASH_SORT(graph.seen, allocation_order);
    size_t count = HASH_COUNT(graph.seen);
    Trial **doomed = dream_raw_calloc(count ? count : 1, sizeof(*doomed));
    if (!doomed) { DREAM_PANIC_LITERAL(u"panic: out of memory ordering cycle destruction"); }
    size_t n = 0;
    HASH_ITER(hh, graph.seen, entry, next) {
        if (!entry->live) {
            doomed[n++] = entry;
            entry->node->dying = 1;
            __atomic_store_n(dream_rc_word(entry->ptr), 0, __ATOMIC_RELEASE);
        }
    }
    for (size_t i = 0; i < n; ++i) { dream_weak_prepare_destroy(doomed[i]->ptr); }
    dream_cycle_leave();
    CycleContext *c = context();
    ++c->finalizing;
    for (size_t i = 0; i < n; ++i) {
        const dream_type_info *info = dream_object_info(doomed[i]->ptr);
        if (info->finalize) { info->finalize(doomed[i]->ptr); }
    }
    --c->finalizing;
    for (size_t i = 0; i < n; ++i) {
        const dream_type_info *info = dream_object_info(doomed[i]->ptr);
        if (info->clear) { info->clear(doomed[i]->ptr); }
    }
    for (size_t i = 0; i < n; ++i) {
        const dream_type_info *info = dream_object_info(doomed[i]->ptr);
        info->reclaim(doomed[i]->ptr);
    }
    dream_cycle_enter();
    HASH_ITER(hh, graph.seen, entry, next) {
        HASH_DEL(graph.seen, entry);
        dream_platform_current->deallocate(entry);
    }
    dream_platform_current->deallocate(doomed);
}

void dream_cycle_drain(void) {
    CycleContext *c = peek_context();
    if (!c || c->draining || c->gate_depth || c->postponed) { return; }
    c->draining = 1;
    dream_cycle_enter();
    while (c->head) {
        Candidate *candidate = c->head;
        c->head = candidate->next;
        if (!c->head) { c->tail = NULL; }
        if (candidate->destroy) {
            dream_cycle_leave();
            candidate->destroy(candidate->ptr);
            dream_cycle_enter();
        } else {
            collect(candidate->node);
            CycleNode *node = candidate->node;
            if (--node->queued == 0 && !node->ptr) { dream_platform_current->deallocate(node); }
        }
        dream_platform_current->deallocate(candidate);
    }
    dream_cycle_leave();
    c->draining = 0;
    release_idle_context(c);
}

int dream_cycle_release(dream_ptr ptr) {
    dream_cycle_enter();
    CycleNode *node = find_node(ptr);
    if (node && !node->dying) {
        int32_t *rc = dream_rc_word(ptr);
        int32_t v = __atomic_load_n(rc, __ATOMIC_RELAXED);
        if (v != DREAM_RC_IMMORTAL) {
            if ((v & INT32_MAX) == 0) { DREAM_PANIC_LITERAL(u"panic: reference count underflow"); }
            int32_t count = (v & INT32_MAX) - 1;
            /* Shared zero is the immortal sentinel, so the last transition must use plain zero. */
            __atomic_store_n(rc, count ? (v & DREAM_RC_SHARED_BIT) | count : 0, __ATOMIC_RELAXED);
            if (may_have_forward_edges || !count) { queue_node(node); }
        }
    }
    dream_cycle_leave();
    dream_cycle_drain();
    return 0;
}

void dream_cycle_postpone(int value) {
    context()->postponed = value;
    if (!value) { dream_cycle_drain(); }
}

#ifdef DREAM_WASM32
__attribute__((export_name("dream_cycle_finish")))
#endif
void dream_cycle_finish(void) {
    dream_cycle_drain();
    CycleContext *c = peek_context();
    if (!c) { return; }
    c->heap_empty = 1;
    release_idle_context(c);
}
