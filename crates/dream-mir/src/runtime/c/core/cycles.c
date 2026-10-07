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
    uint32_t slot;
    struct CycleNode *next_free;
    struct Trial *trial;
#ifdef DREAM_WASM32
    UT_hash_handle hh;
#endif
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

/* Nodes live in fixed chunks so a queued candidate's node pointer survives pool growth; a
 * node's 1-based slot is its stable index. Every access holds the cycle gate. */
#define NODE_CHUNK_BITS 12
#define NODE_CHUNK (1u << NODE_CHUNK_BITS)
static CycleNode **node_chunks;
static uint32_t node_chunk_count;
static uint32_t node_chunk_capacity;
static CycleNode *free_nodes;
static size_t live_nodes;
#ifdef DREAM_WASM32
static CycleNode *nodes;
#endif
static uint64_t next_sequence;
static int may_have_forward_edges;
uint64_t dream_runtime_counters[DREAM_COUNT_LIMIT];

int64_t debug_get_runtime_counter(int32_t id) {
    if (id < 0 || id >= DREAM_COUNT_LIMIT) { return 0; }
    return (int64_t)__atomic_load_n(&dream_runtime_counters[id], __ATOMIC_RELAXED);
}

void dream_cycle_enter(void) {
    CycleContext *c = context();
    if (c->gate_depth++ == 0) {
        dream_platform_current->lock(DREAM_LOCK_CYCLE);
        dream_count(DREAM_COUNT_GATES, 1);
    }
}
void dream_cycle_leave(void) {
    CycleContext *c = context();
    if (--c->gate_depth == 0) {
        c->heap_empty = live_nodes == 0;
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

static void grow_nodes(void) {
    if (node_chunk_count == node_chunk_capacity) {
        uint32_t capacity = node_chunk_capacity ? node_chunk_capacity * 2u : 16u;
        if (capacity > (UINT32_MAX >> NODE_CHUNK_BITS)) { DREAM_PANIC_LITERAL(u"panic: cycle allocation identity exhausted"); }
        CycleNode **chunks = dream_platform_current->resize(node_chunks, (size_t)capacity * sizeof(*chunks));
        if (!chunks) { DREAM_PANIC_LITERAL(u"panic: out of memory indexing cycle objects"); }
        node_chunks = chunks;
        node_chunk_capacity = capacity;
    }
    CycleNode *chunk = dream_raw_calloc(NODE_CHUNK, sizeof(*chunk));
    if (!chunk) { DREAM_PANIC_LITERAL(u"panic: out of memory indexing cycle objects"); }
    uint32_t base = node_chunk_count++ << NODE_CHUNK_BITS;
    node_chunks[base >> NODE_CHUNK_BITS] = chunk;
    for (uint32_t i = NODE_CHUNK; i-- > 0;) {
        chunk[i].slot = base + i + 1u;
        chunk[i].next_free = free_nodes;
        free_nodes = &chunk[i];
    }
}

static CycleNode *take_node(dream_ptr ptr) {
    if (!free_nodes) { grow_nodes(); }
    CycleNode *node = free_nodes;
    free_nodes = node->next_free;
    node->next_free = NULL;
    node->ptr = ptr;
    node->sequence = ++next_sequence;
    node->queued = 0;
    node->dying = 0;
    node->trial = NULL;
    ++live_nodes;
    return node;
}

static void give_node(CycleNode *node) {
    node->next_free = free_nodes;
    free_nodes = node;
}

static CycleNode *find_node(dream_ptr ptr) {
    dream_count(DREAM_COUNT_LOOKUPS, 1);
#ifdef DREAM_WASM32
    CycleNode *node;
    HASH_FIND(hh, nodes, &ptr, sizeof(ptr), node);
    return node;
#else
    uint32_t slot = *dream_cycle_slot(ptr) - 1u;
    return slot == UINT32_MAX ? NULL : &node_chunks[slot >> NODE_CHUNK_BITS][slot & (NODE_CHUNK - 1u)];
#endif
}

static void unlink_node(CycleNode *node) {
#ifdef DREAM_WASM32
    HASH_DEL(nodes, node);
#else
    *dream_cycle_slot(node->ptr) = 0;
#endif
    --live_nodes;
}

void dream_cycle_register(dream_ptr ptr) {
    if (dream_rc_immortal(ptr)) { return; }
    dream_cycle_enter();
    int32_t tag = dream_object_tag(ptr);
    if (tag == TAG_FUTURE || tag == TAG_FUNCBOX || tag == TAG_CLOSURE_ENV || tag == TAG_ARRAY) {
        may_have_forward_edges = 1;
    }
    if (!find_node(ptr)) {
        if (next_sequence == UINT64_MAX) { DREAM_PANIC_LITERAL(u"panic: cycle allocation identity exhausted"); }
        CycleNode *node = take_node(ptr);
#ifdef DREAM_WASM32
        HASH_ADD(hh, nodes, ptr, sizeof(ptr), node);
#else
        *dream_cycle_slot(ptr) = node->slot;
#endif
        dream_count(DREAM_COUNT_CYCLE_NODES, 1);
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

void dream_cycle_forget_slow(dream_ptr ptr) {
    dream_cycle_enter();
    CycleNode *node = find_node(ptr);
    if (node) {
        unlink_node(node);
        dream_count(DREAM_COUNT_CYCLE_FORGETS, 1);
        if (!live_nodes) { may_have_forward_edges = 0; }
        node->ptr = 0;
        if (!node->queued) { give_node(node); }
    }
    dream_cycle_leave();
}

/* Queue entries recycle through a gate-guarded free list: teardown of a tracked graph queues
 * one entry per object. */
static Candidate *free_candidates;

static Candidate *take_candidate(void) {
    Candidate *candidate = free_candidates;
    if (candidate) {
        free_candidates = candidate->next;
        *candidate = (Candidate){0};
        return candidate;
    }
    candidate = dream_raw_calloc(1, sizeof(*candidate));
    if (!candidate) { DREAM_PANIC_LITERAL(u"panic: out of memory queuing cycle candidate"); }
    return candidate;
}

static void give_candidate(Candidate *candidate) {
    candidate->next = free_candidates;
    free_candidates = candidate;
}

static void queue_node(CycleNode *node) {
    Candidate *candidate = take_candidate();
    CycleContext *c = context();
    candidate->node = node;
    ++node->queued;
    dream_count(DREAM_COUNT_CANDIDATES, 1);
    if (c->tail) { c->tail->next = candidate; } else { c->head = candidate; }
    c->tail = candidate;
}

int dream_cycle_defer_destroy(dream_ptr ptr, void (*destroy)(dream_ptr)) {
    CycleContext *c = peek_context();
    if (!c || !c->gate_depth) { return 0; }
    Candidate *candidate = take_candidate();
    dream_weak_prepare_destroy(ptr);
    candidate->ptr = ptr;
    candidate->destroy = destroy;
    dream_count(DREAM_COUNT_DEFERRED, 1);
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

void dream_cycle_check_store_slow(dream_ptr owner, dream_ptr child) {
    dream_cycle_enter();
    CycleNode *a = owner ? find_node(owner) : NULL;
    CycleNode *b = child ? find_node(child) : NULL;
    if ((a && a->dying) || (b && b->dying)) {
        DREAM_PANIC_LITERAL(u"panic: publication or mutation of a dying object");
    }
    if (a && b && b->sequence >= a->sequence) { may_have_forward_edges = 1; }
    dream_cycle_leave();
}

__attribute__((noinline)) void dream_cycle_store_begin_slow(dream_ptr owner, dream_ptr child, int value, int tracked) {
    dream_cycle_enter();
    if (tracked) { dream_cycle_check_store_slow(owner, child); }
    if (value == 2) { may_have_forward_edges = 1; }
}
__attribute__((noinline)) void dream_cycle_store_end_slow(void) {
    dream_cycle_leave();
    dream_cycle_drain();
}

/* A trial entry hangs off its collector node for the duration of one gated trial, so membership
 * is a pointer test; entries recycle through a gate-guarded pool. */
typedef struct Trial {
    dream_ptr ptr;
    CycleNode *node;
    int64_t residual;
    int live;
    struct Trial *next;
    struct Trial *member;
} Trial;

typedef struct {
    Trial *members;
    Trial *last_member;
    Trial *head;
    Trial *tail;
    int mode;
} TrialGraph;

static Trial *free_trials;

static Trial *take_trial(void) {
    Trial *entry = free_trials;
    if (entry) {
        free_trials = entry->member;
        *entry = (Trial){0};
        return entry;
    }
    entry = dream_raw_calloc(1, sizeof(*entry));
    if (!entry) { DREAM_PANIC_LITERAL(u"panic: out of memory constructing cycle trial"); }
    return entry;
}

static void trial_edge(dream_ptr ptr, void *arg) {
    TrialGraph *graph = arg;
    dream_count(DREAM_COUNT_EDGES, 1);
    CycleNode *node = find_node(ptr);
    if (!node || node->dying) { return; }
    Trial *entry = node->trial;
    if (graph->mode == 0 && !entry) {
        entry = take_trial();
        entry->ptr = ptr;
        entry->node = node;
        entry->residual = dream_rc_count(ptr);
        node->trial = entry;
        if (graph->last_member) { graph->last_member->member = entry; } else { graph->members = entry; }
        graph->last_member = entry;
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

/* Bottom-up merge sort by allocation identity: destruction order is part of the observable
 * cleanup trace, and the freestanding wasm runtime has no qsort. */
static Trial *sort_by_allocation(Trial *list, size_t count) {
    for (size_t width = 1; width < count; width *= 2) {
        Trial *result = NULL, **tail = &result, *rest = list;
        while (rest) {
            Trial *left = rest, *right = rest;
            size_t left_n = 0, right_n = 0;
            while (right && left_n < width) { right = right->next; ++left_n; }
            rest = right;
            while (rest && right_n < width) { rest = rest->next; ++right_n; }
            while (left_n || right_n) {
                int take_left = left_n && (!right_n || left->node->sequence < right->node->sequence);
                Trial *pick = take_left ? left : right;
                if (take_left) { left = left->next; --left_n; } else { right = right->next; --right_n; }
                *tail = pick;
                tail = &pick->next;
            }
        }
        *tail = NULL;
        list = result;
    }
    return list;
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
    dream_count(DREAM_COUNT_TRIALS, 1);
    trial_edge(candidate->ptr, &graph);
    for (Trial *entry = graph.head; entry; entry = entry->next) {
        dream_visit_owned(entry->ptr, trial_edge, &graph);
    }
    graph.mode = 1;
    for (Trial *entry = graph.members; entry; entry = entry->member) {
        dream_visit_owned(entry->ptr, trial_edge, &graph);
    }
    graph.head = graph.tail = NULL;
    graph.mode = 2;
    for (Trial *entry = graph.members; entry; entry = entry->member) {
        if (entry->residual < 0) { DREAM_PANIC_LITERAL(u"panic: invalid strong-edge ownership count"); }
        if (entry->residual > 0) { trial_edge(entry->ptr, &graph); }
    }
    while (graph.head) {
        Trial *entry = graph.head;
        graph.head = entry->next;
        if (!graph.head) { graph.tail = NULL; }
        dream_visit_owned(entry->ptr, trial_edge, &graph);
    }
    Trial *doomed_list = NULL, **doomed_tail = &doomed_list;
    size_t n = 0;
    for (Trial *entry = graph.members; entry; entry = entry->member) {
        entry->node->trial = NULL;
        if (!entry->live) {
            *doomed_tail = entry;
            doomed_tail = &entry->next;
            ++n;
            entry->node->dying = 1;
            __atomic_store_n(dream_rc_word(entry->ptr), 0, __ATOMIC_RELEASE);
        }
    }
    *doomed_tail = NULL;
    dream_ptr *doomed = NULL;
    if (n) {
        doomed = dream_raw_calloc(n, sizeof(*doomed));
        if (!doomed) { DREAM_PANIC_LITERAL(u"panic: out of memory ordering cycle destruction"); }
        size_t i = 0;
        for (Trial *entry = sort_by_allocation(doomed_list, n); entry; entry = entry->next) { doomed[i++] = entry->ptr; }
    }
    if (graph.last_member) {
        graph.last_member->member = free_trials;
        free_trials = graph.members;
    }
    if (!n) { return; }
    for (size_t i = 0; i < n; ++i) { dream_weak_prepare_destroy(doomed[i]); }
    dream_cycle_leave();
    CycleContext *c = context();
    ++c->finalizing;
    for (size_t i = 0; i < n; ++i) {
        const dream_type_info *info = dream_object_info(doomed[i]);
        if (info->finalize) { info->finalize(doomed[i]); }
    }
    --c->finalizing;
    for (size_t i = 0; i < n; ++i) {
        const dream_type_info *info = dream_object_info(doomed[i]);
        if (info->clear) { info->clear(doomed[i]); }
    }
    for (size_t i = 0; i < n; ++i) {
        const dream_type_info *info = dream_object_info(doomed[i]);
        info->reclaim(doomed[i]);
    }
    dream_cycle_enter();
    dream_platform_current->deallocate(doomed);
}

void dream_cycle_drain(void) {
    CycleContext *c = peek_context();
    if (!c || c->draining || c->gate_depth || c->postponed) { return; }
    if (!c->head) {
        release_idle_context(c);
        return;
    }
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
            if (--node->queued == 0 && !node->ptr) { give_node(node); }
        }
        give_candidate(candidate);
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
            if (count && may_have_forward_edges) { dream_count(DREAM_COUNT_FORWARD_QUEUES, 1); }
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
