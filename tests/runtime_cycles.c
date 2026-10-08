#include "dream_core.h"
#include "dream_region.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
#include <unistd.h>

typedef struct { dream_ptr edge; int id; uintptr_t weak; } Node;
static int trace[16], trace_count;
void dream_panic(dream_ptr message) { (void)message; abort(); }
void dream_future_fini(dream_ptr p) { (void)p; abort(); }
static void visit(dream_ptr p) { dream_visit_edge(((Node *)p)->edge); }
static void finalize(dream_ptr p) {
    Node *n = (Node *)p;
    trace[trace_count++] = n->id;
    if (n->edge) { assert(((Node *)n->edge)->id > 0); }
    if (n->weak) { assert(weakDead(n->weak)); assert(!weakLoad(n->weak)); }
}
static void clear(dream_ptr p) { dream_ptr child = ((Node *)p)->edge; ((Node *)p)->edge = 0; dream_release(child); }
static const dream_type_info private_info = {visit, finalize, clear, dream_recycle, 0, 0};
static const dream_type_info node_info = {visit, finalize, clear, dream_recycle, 1, 0};
static const dream_type_info pure_node_info = {visit, NULL, clear, dream_recycle, 1, 1};
const dream_type_info *dream_type_info_for_tag(int32_t tag) { return tag == TAG_STRUCT_BASE ? &node_info : dream_builtin_type_info(tag); }
static dream_ptr node(int id) { dream_ptr p = dream_malloc(sizeof(Node), TAG_STRUCT_BASE); memset(p, 0, sizeof(Node)); ((Node *)p)->id = id; return p; }
static void edge(dream_ptr a, dream_ptr b) {
    int gate = dream_cycle_store_begin(a, b, 0);
    dream_retain(b);
    dream_ptr old = ((Node *)a)->edge;
    ((Node *)a)->edge = b;
    dream_release(old);
    dream_cycle_store_end(gate);
}
static void array_visit(dream_ptr p) {
    for (int32_t i = 0; i < dream_i32(p)[0]; ++i) {
        dream_ptr child; memcpy(&child, (char *)p + 4 + (size_t)i * sizeof(child), sizeof(child));
        dream_visit_edge(child);
    }
}
static void array_clear(dream_ptr p) {
    for (int32_t i = 0; i < dream_i32(p)[0]; ++i) {
        dream_ptr child, zero = 0; char *slot = (char *)p + 4 + (size_t)i * sizeof(child);
        memcpy(&child, slot, sizeof(child)); memcpy(slot, &zero, sizeof(zero)); dream_release(child);
    }
}
static const dream_type_info array_info = {array_visit, NULL, array_clear, dream_recycle, 1, 1};
static void array_growth(void) {
    int64_t before = debug_get_live_objects();
    trace_count = 0;
    dream_ptr owner = node(1), arr = dream_array_new(1, sizeof(dream_ptr));
    dream_set_type(arr, &array_info);
    dream_retain(owner); memcpy((char *)arr + 4, &owner, sizeof(owner));
    edge(owner, arr);
    dream_ptr grown = dream_array_realloc_rc(arr, 100, sizeof(dream_ptr), dream_release);
    assert(grown != arr && dream_object_info(grown) == &array_info);
    assert(dream_rc_count(owner) == 3);
    dream_release(owner);
    dream_release(grown);
    assert(trace_count == 1 && debug_get_live_objects() == before);

    trace_count = 0;
    owner = node(1); arr = dream_array_new(1, sizeof(dream_ptr)); dream_set_type(arr, &array_info);
    dream_retain(owner); memcpy((char *)arr + 4, &owner, sizeof(owner));
    grown = dream_array_realloc_rc(arr, 100, sizeof(dream_ptr), dream_release);
    assert(dream_rc_count(owner) == 2);
    dream_release(owner); dream_release(grown);
    assert(trace_count == 1 && debug_get_live_objects() == before);
}

static int delayed_finalized;
static void delayed_destroy(dream_ptr ptr) {
    assert(!dream_cycle_defer_destroy(ptr, delayed_destroy));
    ++delayed_finalized;
    dream_recycle(ptr);
}
static void gated_finalization(void) {
    dream_ptr leaf = dream_malloc(8, 0);
    uintptr_t weak = weakBind(leaf);
    dream_cycle_enter();
    __atomic_store_n(dream_rc_word(leaf), 0, __ATOMIC_RELEASE);
    assert(dream_cycle_defer_destroy(leaf, delayed_destroy));
    assert(!delayed_finalized && weakDead(weak));
    dream_cycle_leave();
    dream_cycle_drain();
    assert(delayed_finalized == 1);
    weakReleaseRaw(weak);
}

static void pending_release(dream_ptr ptr) { dream_release(ptr); }
static void gated_owned_release(void) {
    dream_ptr leaf = dream_malloc(8, 0);
    uintptr_t weak = weakBind(leaf);
    dream_retain(leaf);
    dream_cycle_enter();
    assert(dream_cycle_defer_destroy(leaf, pending_release));
    assert(!weakDead(weak));
    dream_cycle_leave();
    dream_cycle_drain();
    assert(dream_rc_count(leaf) == 1 && !weakDead(weak));
    dream_release(leaf);
    assert(weakDead(weak));
    weakReleaseRaw(weak);
}

static void erased_nonlast_release(void) {
    int64_t before = debug_get_live_objects();
    for (int shared = 0; shared < 2; ++shared) {
        dream_ptr leaf = dream_malloc(8, 0);
        uintptr_t weak = weakBind(leaf);
        dream_retain(leaf);
        if (shared) { dream_publish(leaf); }
        dream_cycle_enter();
        assert(dream_release_nonlast(leaf));
        assert(dream_rc_count(leaf) == 1 && !weakDead(weak));
        assert(!dream_release_nonlast(leaf));
        dream_cycle_leave();
        dream_release(leaf);
        assert(weakDead(weak));
        weakReleaseRaw(weak);
    }
    dream_ptr managed = node(1);
    dream_retain(managed);
    assert(!dream_release_nonlast(managed) && dream_rc_count(managed) == 2);
    dream_release(managed); dream_release(managed);
    assert(debug_get_live_objects() == before);
}

static void pure_zero_count_chain(void) {
    int64_t before = debug_get_live_objects();
    dream_ptr root = node(1), tail = root;
    dream_set_type(root, &pure_node_info);
    for (int i = 0; i < 10000; ++i) {
        dream_ptr child = node(2);
        dream_set_type(child, &pure_node_info);
        edge(tail, child);
        dream_release(child);
        tail = child;
    }
    uintptr_t weak = weakBind(root);
    int64_t gates = debug_get_runtime_counter(DREAM_COUNT_GATES);
    dream_release(root);
    assert(debug_get_runtime_counter(DREAM_COUNT_GATES) == gates + 2);
    assert(weakDead(weak));
    weakReleaseRaw(weak);
    assert(debug_get_live_objects() == before);
}

static pthread_mutex_t barrier_mu = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t barrier_cv = PTHREAD_COND_INITIALIZER;
static int arrived;
static void *drop_owner(void *arg) {
    pthread_mutex_lock(&barrier_mu);
    if (++arrived == 2) { pthread_cond_broadcast(&barrier_cv); }
    while (arrived != 2) { pthread_cond_wait(&barrier_cv, &barrier_mu); }
    pthread_mutex_unlock(&barrier_mu);
    dream_release(arg);
    return NULL;
}
static dream_ptr immortal_root;
static void immortal_cycle(void) {
    int64_t before = debug_get_live_objects();
    trace_count = 0;
    immortal_root = node(1);
    edge(immortal_root, immortal_root);
    uintptr_t weak = weakBind(immortal_root);
    dream_pin_immortal(immortal_root);
    dream_retain(immortal_root);
    assert(dream_rc_immortal(immortal_root));
    dream_release(immortal_root);
    assert(!weakDead(weak));
    assert(weakLoad(weak) == immortal_root);
    dream_release(immortal_root);
    dream_pin_immortal(immortal_root);
    weakReleaseRaw(weak);
    assert(trace_count == 0 && debug_get_live_objects() == before);
}
void dream_region_enter(void);
void dream_region_leave(void);
static int64_t cycle_nodes(void) { return debug_get_runtime_counter(DREAM_COUNT_CYCLE_NODES); }
static dream_ptr private_node(int id) {
    dream_ptr p = dream_malloc_private(sizeof(Node), TAG_STRUCT_BASE, &private_info);
    memset(p, 0, sizeof(Node)); ((Node *)p)->id = id; return p;
}
static void private_region(void) {
    int64_t before = debug_get_live_objects(), nodes = cycle_nodes();
    trace_count = 0;
    dream_region_enter();
    dream_ptr a = private_node(1), b = private_node(2);
    edge(a, b); dream_release(b);
    assert(dream_region_owns(a) && dream_object_info(a) == &private_info);
    assert(dream_object_tag(a) == TAG_STRUCT_BASE);
    dream_ptr tracked = node(3);
    assert(!dream_region_owns(tracked) && dream_object_info(tracked) == &node_info);
    assert(cycle_nodes() == nodes + 1);
    dream_release(tracked);
    dream_region_leave();
    assert(trace_count == 1 && debug_get_live_objects() == before);

    dream_ptr outside = private_node(4);
    assert(dream_object_info(outside) == &node_info && dream_object_tag(outside) == TAG_STRUCT_BASE);
    assert(cycle_nodes() == nodes + 2);
    edge(outside, outside); dream_release(outside);
    assert(trace_count == 2 && debug_get_live_objects() == before);
}

static void forest_components(void) {
    int64_t before = debug_get_live_objects();
    trace_count = 0;
    dream_ptr unrelated = node(3);
    edge(unrelated, unrelated);
    dream_ptr parent = node(1), child = node(2);
    int observing = dream_cycle_store_begin(child, parent, 3);
    uintptr_t weak = weakBind(parent);
    ((Node *)child)->weak = weak;
    dream_cycle_store_end(observing);
    int64_t queues = debug_get_runtime_counter(DREAM_COUNT_FORWARD_QUEUES);
    int64_t trials = debug_get_runtime_counter(DREAM_COUNT_TRIALS);
    edge(parent, child);
    dream_release(child);
    assert(!trace_count && !weakDead(weak));
    assert(debug_get_runtime_counter(DREAM_COUNT_FORWARD_QUEUES) == queues);
    assert(debug_get_runtime_counter(DREAM_COUNT_TRIALS) == trials);
    dream_release(parent);
    assert(trace_count == 2 && trace[0] == 1 && trace[1] == 2);
    assert(weakDead(weak));
    weakReleaseRaw(weak);
    dream_release(unrelated);
    assert(trace_count == 3 && debug_get_live_objects() == before);
}

static void unknown_store(void) {
    int64_t before = debug_get_live_objects();
    trace_count = 0;
    dream_ptr a = node(1), b = node(2);
    edge(a, b);
    int64_t unknown = debug_get_runtime_counter(DREAM_COUNT_UNKNOWN_STORES);
    int gate = dream_cycle_store_begin(0, 0, 2);
    dream_retain(a);
    ((Node *)b)->edge = a;
    dream_cycle_store_end(gate);
    assert(debug_get_runtime_counter(DREAM_COUNT_UNKNOWN_STORES) == unknown + 1);
    dream_release(a); dream_release(b);
    assert(trace_count == 2 && trace[0] == 1 && trace[1] == 2);
    assert(debug_get_live_objects() == before);

    trace_count = 0;
    a = node(4);
    gate = dream_cycle_store_begin(0, 0, 2);
    dream_retain(a);
    ((Node *)a)->edge = a;
    dream_cycle_store_end(gate);
    dream_release(a);
    assert(trace_count == 1 && trace[0] == 4);
    assert(debug_get_live_objects() == before);

    trace_count = 0;
    a = node(5);
    int64_t trials = debug_get_runtime_counter(DREAM_COUNT_TRIALS);
    dream_retain(a); dream_release(a);
    assert(!trace_count && debug_get_runtime_counter(DREAM_COUNT_TRIALS) == trials);
    dream_release(a);
    assert(trace_count == 1 && debug_get_live_objects() == before);
}

static void batched_construction(void) {
    trace_count = 0;
    int64_t before = debug_get_live_objects();
    int64_t gates = debug_get_runtime_counter(DREAM_COUNT_GATES);
    int gate = dream_cycle_construction_begin(0);
    dream_ptr parent = node(1), child = node(2);
    int nested = dream_cycle_construction_begin(0);
    assert(nested == 2);
    edge(parent, child);
    dream_cycle_store_end(nested);
    dream_cycle_initialize_edge(parent, 0);
    dream_release(child);
    assert(!trace_count);
    dream_cycle_store_end(gate);
    assert(debug_get_runtime_counter(DREAM_COUNT_GATES) == gates + 1);
    dream_release(parent);
    assert(trace_count == 2 && debug_get_live_objects() == before);
}

static void component_mutations(void) {
    int64_t before = debug_get_live_objects();
    for (int iteration = 0; iteration < 10000; ++iteration) {
        trace_count = 0;
        dream_ptr a = node(1), b = node(2), shared = node(3);
        edge(a, shared); edge(b, shared);
        dream_release(shared);
        edge(a, b);
        assert(trace_count == 0);
        edge(b, a);
        assert(trace_count == 1 && trace[0] == 3);
        dream_release(a);
        assert(trace_count == 1);
        edge(b, 0);
        assert(trace_count == 2 && trace[1] == 1);
        dream_release(b);
        assert(trace_count == 3 && debug_get_live_objects() == before);
    }
}
static void opaque_recycling(void) {
    int64_t before = debug_get_live_objects();
    dream_ptr value = dream_malloc(32, 0);
    dream_free(value);
    dream_ptr reused = dream_malloc(32, 0);
    assert(reused == value);
    dream_free(reused);
    assert(debug_get_live_objects() == before);
}

static void cyclic_metadata_recycling(void) {
    int64_t before = debug_get_live_objects();
    for (int round = 0; round < 64; ++round) {
        dream_ptr values[32];
        int gate = dream_cycle_construction_begin(0);
        for (size_t i = 0; i < 32; ++i) {
            values[i] = node((int)i + 1);
            dream_set_type(values[i], &pure_node_info);
        }
        for (size_t i = 0; i < 32; ++i) { edge(values[i], values[(i + 1) % 32]); }
        dream_cycle_store_end(gate);
        for (size_t i = 0; i < 31; ++i) {
            dream_release(values[i]);
            assert(debug_get_live_objects() == before + 32);
        }
        dream_release(values[31]);
        assert(debug_get_live_objects() == before);
    }
}

static void graph_construction_scopes(void) {
    int64_t baseline = debug_get_live_objects();
    for (int round = 0; round < 100; ++round) {
        trace_count = 0;
        int64_t trials = debug_get_runtime_counter(DREAM_COUNT_TRIALS);
        int scope = dream_cycle_graph_begin(0);
        dream_ptr root = node(1);
        for (int id = 2; id <= 8; ++id) {
            int nested = dream_cycle_graph_begin(0);
            dream_ptr parent = node(id);
            dream_cycle_initialize_edge(parent, root);
            dream_retain(root);
            ((Node *)parent)->edge = root;
            dream_release(root);
            root = parent;
            dream_cycle_graph_end(nested);
        }
        dream_ptr discarded = node(99);
        dream_release(discarded);
        assert(trace_count == 0);
        dream_cycle_graph_end(scope);
        assert(trace_count == 1 && trace[0] == 99);
        assert(debug_get_runtime_counter(DREAM_COUNT_TRIALS) == trials);
        trace_count = 0;
        dream_release(root);
        assert(trace_count == 8);
        assert(debug_get_live_objects() == baseline);
    }
    trace_count = 0;
    int scope = dream_cycle_graph_begin(0);
    dream_ptr a = node(1), b = node(2);
    dream_cycle_initialize_edge(b, a);
    dream_retain(a);
    ((Node *)b)->edge = a;
    dream_cycle_graph_end(scope);
    edge(a, b);
    dream_release(a);
    dream_release(b);
    assert(trace_count == 2 && trace[0] == 1 && trace[1] == 2);
    assert(debug_get_live_objects() == baseline);
}

static void moved_owners_close_a_cycle(void) {
    int64_t baseline = debug_get_live_objects();
    trace_count = 0;
    dream_ptr a = node(1), b = node(2);
    int gate = dream_cycle_store_begin(a, b, 0);
    ((Node *)a)->edge = b;
    dream_cycle_store_end(gate);
    /* Both owning tokens move into fields; no decrement follows the closing store. */
    gate = dream_cycle_store_begin(b, a, 0);
    ((Node *)b)->edge = a;
    dream_cycle_store_end(gate);
    assert(trace_count == 2 && trace[0] == 1 && trace[1] == 2);
    assert(debug_get_live_objects() == baseline);
}

int main(void) {
    alarm(30);
    opaque_recycling();
    cyclic_metadata_recycling();
    component_mutations();
    trace_count = 0;
    int64_t baseline = debug_get_live_objects();
    dream_ptr a = node(1);
    dream_publish(a); dream_release(a);
    assert(trace_count == 1 && debug_get_live_objects() == baseline);
    trace_count = 0;
    a = node(1);
    edge(a, a); dream_release(a);
    assert(trace_count == 1 && trace[0] == 1);
    assert(debug_get_live_objects() == baseline);
    trace_count = 0;
    a = node(1); dream_ptr b = node(2);
    edge(a,b); edge(b,a);
    uintptr_t weak = weakBind(a); ((Node *)a)->weak = weak;
    dream_release(a); assert(trace_count == 0);
    dream_release(b);
    assert(trace_count == 2 && trace[0] == 1 && trace[1] == 2);
    assert(weakDead(weak)); weakReleaseRaw(weak);
    assert(debug_get_live_objects() == baseline);
    trace_count = 0;
    a = node(1); b = node(2); dream_ptr root = node(3);
    edge(a,b); edge(b,a); edge(root,a);
    dream_release(a); dream_release(b); assert(trace_count == 0);
    edge(root,0); assert(trace_count == 2);
    dream_release(root); assert(trace_count == 3);
    assert(debug_get_live_objects() == baseline);
    array_growth();
    gated_finalization();
    gated_owned_release();
    erased_nonlast_release();
    pure_zero_count_chain();
    forest_components();
    unknown_store();
    batched_construction();
    graph_construction_scopes();
    moved_owners_close_a_cycle();
    for (int round = 0; round < 1000; ++round) {
        trace_count = 0; arrived = 0;
        a = node(1); b = node(2); edge(a,b); edge(b,a);
        dream_publish(a);
        pthread_t first, second;
        assert(pthread_create(&first, NULL, drop_owner, a) == 0);
        assert(pthread_create(&second, NULL, drop_owner, b) == 0);
        pthread_join(first, NULL); pthread_join(second, NULL);
        assert(trace_count == 2 && trace[0] == 1 && trace[1] == 2);
        assert(debug_get_live_objects() == baseline);
    }
    immortal_cycle();
    private_region();
    puts("cycle trial deletion passed");
}

void dream_release_object(dream_ptr ptr) { dream_release(ptr); }
