#include "dream_core.h"
#include "dream_platform_internal.h"

#define uthash_malloc(size) dream_platform_current->allocate(size)
#define uthash_free(ptr, size) dream_platform_current->deallocate(ptr)
#define uthash_fatal(msg) DREAM_PANIC_LITERAL(u"panic: out of memory indexing cycle objects")
#include "uthash.h"
#include "cycle_components.h"

typedef struct CycleNode {
    dream_ptr ptr;
    uint64_t sequence;
    unsigned queued : 31;
    unsigned dying : 1;
    uint32_t slot;
    /* A pooled slot has no trial membership; a live slot is never on the free list. */
    union {
        struct CycleNode *next_free;
        struct Trial *trial;
    };
    CycleComponent *component;
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
    CycleComponent *construction_component;
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
    if (c && c->heap_empty && !c->gate_depth && !c->draining && !c->postponed && !c->finalizing && !c->head && !c->edge && !c->construction_component) {
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
static uint64_t unknown_store_sequence;
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

int dream_cycle_construction_begin(int private_graph) {
    CycleContext *c = peek_context();
    if (c && c->gate_depth) { return 2; }
    if (private_graph && dream_region_active()) { return 0; }
    dream_cycle_enter();
    return 1;
}
#include "cycle_construction.h"
int dream_cycle_acquire(void) {
    if (context()->gate_depth) { return 0; }
    dream_cycle_enter();
    return 1;
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
static const dream_type_info string_info = { string_visit, NULL, NULL, NULL, 0, 0};
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
static const dream_type_info closure_info = { closure_visit, NULL, closure_clear, dream_recycle, 1, 1};
static const dream_type_info environment_info = { environment_visit, NULL, environment_clear, dream_recycle, 1, 1};
const dream_type_info *dream_builtin_type_info(int32_t tag) {
    switch (tag) {
    case TAG_STRING: return &string_info;
    case TAG_FUNCBOX: return &closure_info;
    case TAG_CLOSURE_ENV: return &environment_info;
    default: return NULL;
    }
}

static __attribute__((noinline, cold)) void grow_nodes(void) {
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
    int32_t tag = dream_object_tag(ptr);
    int dynamic = tag == TAG_FUTURE || tag == TAG_FUNCBOX || tag == TAG_CLOSURE_ENV || tag == TAG_ARRAY;
    node->component = construction_component(dynamic);
    ++live_nodes;
    return node;
}

static void give_node(CycleNode *node) {
    component_drop(node->component);
    node->component = NULL;
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
    int acquired = dream_cycle_acquire();
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
    if (acquired) { dream_cycle_leave(); }
}

void dream_cycle_register_new(dream_ptr ptr) {
    int acquired = dream_cycle_acquire();
    if (next_sequence == UINT64_MAX) { DREAM_PANIC_LITERAL(u"panic: cycle allocation identity exhausted"); }
    CycleNode *node = take_node(ptr);
#ifdef DREAM_WASM32
    HASH_ADD(hh, nodes, ptr, sizeof(ptr), node);
#else
    *dream_cycle_slot(ptr) = node->slot;
#endif
    dream_count(DREAM_COUNT_CYCLE_NODES, 1);
    if (acquired) { dream_cycle_leave(); }
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
    int acquired = dream_cycle_acquire();
    CycleNode *node = find_node(ptr);
    if (node) {
        unlink_node(node);
        dream_count(DREAM_COUNT_CYCLE_FORGETS, 1);
        node->ptr = 0;
        if (!node->queued) { give_node(node); }
    }
    if (acquired) { dream_cycle_leave(); }
}

#include "cycle_scratch.h"

static void queue_node(CycleNode *node) {
    if (node->queued == INT32_MAX) { DREAM_PANIC_LITERAL(u"panic: cycle candidate reference overflow"); }
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
    // Erased release dispatch can be deferred before it selects/decrements the count.
    // Its queued reference still owns the object; only a claimed zero invalidates observers.
    if (dream_rc_count(ptr) == 0) { dream_weak_prepare_destroy(ptr); }
    candidate->ptr = ptr;
    candidate->destroy = destroy;
    dream_count(DREAM_COUNT_DEFERRED, 1);
    if (c->tail) { c->tail->next = candidate; } else { c->head = candidate; }
    c->tail = candidate;
    return 1;
}

void dream_cycle_retain(dream_ptr ptr) {
    int acquired = dream_cycle_acquire();
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
    if (acquired) { dream_cycle_leave(); }
}

static int node_possible(CycleNode *node) {
    return node->component ? component_possible(node->component) : node->sequence <= unknown_store_sequence;
}

#include "cycle_edges.h"

void dream_cycle_initialize_edge(dream_ptr owner, dream_ptr child) {
    if (!child) { return; }
    CycleContext *c = context();
    if (!c->gate_depth) { DREAM_PANIC_LITERAL(u"panic: initialization without ownership gate"); }
    /* Verified fresh builders import no reference owner and run no application code.
     * Registration has already joined these nodes; a new owner's edges cannot close a cycle. */
    if (c->construction_component && owner != child) { return; }
    check_store_locked(owner, child, 2);
}

static void check_store(dream_ptr owner, dream_ptr child, int strong) {
    dream_cycle_enter();
    check_store_locked(owner, child, strong);
    dream_cycle_leave();
}

void dream_cycle_check_store_slow(dream_ptr owner, dream_ptr child) {
    check_store(owner, child, 1);
}

__attribute__((noinline)) int dream_cycle_store_begin_slow(dream_ptr owner, dream_ptr child, int value, int tracked) {
    /* Token 2 borrows an outer gate; only token 1 owns an acquisition to release. */
    int token = context()->gate_depth ? 2 : 1;
    if (token == 1) { dream_cycle_enter(); }
    if (tracked) { check_store_locked(owner, child, value != 3); }
    if (value == 2) {
        CycleNode *node = owner ? find_node(owner) : NULL;
        if (node) {
            if (!node->component) { node->component = component_take(1); }
            else { component_root(node->component)->uncertain = 1; }
            queue_node(node);
        } else {
            component_invalidate_all();
            unknown_store_sequence = next_sequence;
            CycleNode *child_node = child ? find_node(child) : NULL;
            if (child_node) { queue_node(child_node); }
        }
    }
    return token;
}
__attribute__((noinline)) void dream_cycle_store_end_slow(void) {
    dream_cycle_leave();
    dream_cycle_drain();
}

static void collect(CycleNode *candidate) {
    TrialGraph graph = {0};
    if (!candidate->ptr || candidate->dying) { return; }
    // Distinct forest components cannot form a directed cycle until an edge closes a path.
    if (!node_possible(candidate) && dream_rc_count(candidate->ptr) > 0) { return; }
    // A zero-count object has no incoming strong edge and cannot be part of a cycle.
    // Keep its teardown iterative without allocating trial counts or ordering scratch.
    if (dream_rc_count(candidate->ptr) == 0) {
        dream_ptr ptr = candidate->ptr;
        const dream_type_info *info = dream_object_info(ptr);
        candidate->dying = 1;
        dream_weak_prepare_destroy(ptr);
        if (!info->finalize && info->clear_no_user_code) {
            if (info->clear) { info->clear(ptr); }
            info->reclaim(ptr);
            return;
        }
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
    int acquired = dream_cycle_acquire();
    CycleNode *node = find_node(ptr);
    if (node && !node->dying) {
        int32_t *rc = dream_rc_word(ptr);
        int32_t v = __atomic_load_n(rc, __ATOMIC_RELAXED);
        if (v != DREAM_RC_IMMORTAL) {
            if ((v & INT32_MAX) == 0) { DREAM_PANIC_LITERAL(u"panic: reference count underflow"); }
            int32_t count = (v & INT32_MAX) - 1;
            /* Shared zero is the immortal sentinel, so the last transition must use plain zero. */
            __atomic_store_n(rc, count ? (v & DREAM_RC_SHARED_BIT) | count : 0, __ATOMIC_RELAXED);
            if (!count) { queue_node(node); }
            else if (node_possible(node)) {
                dream_count(DREAM_COUNT_FORWARD_QUEUES, 1);
                queue_node(node);
            }
        }
    }
    if (acquired) { dream_cycle_leave(); }
    if (acquired) { dream_cycle_drain(); }
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
