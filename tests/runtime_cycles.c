#include "dream_core.h"
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
static const dream_type_info node_info = {visit, finalize, clear, dream_recycle, 1};
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
static const dream_type_info array_info = {array_visit, NULL, array_clear, dream_recycle, 1};
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
    assert(dream_cycle_defer_destroy(leaf, delayed_destroy));
    assert(!delayed_finalized && weakDead(weak));
    dream_cycle_leave();
    dream_cycle_drain();
    assert(delayed_finalized == 1);
    weakReleaseRaw(weak);
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
int main(void) {
    alarm(30);
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
    puts("cycle trial deletion passed");
}

void dream_release_object(dream_ptr ptr) { dream_release(ptr); }
