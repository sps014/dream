#include "dream_core.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct { dream_ptr edge; int id; } Node;
static int trace[16], count;
void dream_panic(dream_ptr message) { (void)message; abort(); }
void dream_future_fini(dream_ptr p) { (void)p; abort(); }
static void visit(dream_ptr p) { dream_visit_edge(((Node *)dream_p(p))->edge); }
static void finalize(dream_ptr p) { trace[count++] = ((Node *)dream_p(p))->id; }
void dream_release_object(dream_ptr p);
static void clear(dream_ptr p) {
    Node *n = dream_p(p);
    dream_ptr child = n->edge;
    n->edge = 0;
    dream_release_object(child);
}
static const dream_type_info info = { visit, finalize, clear, dream_recycle };
const dream_type_info *dream_type_info_for_tag(int32_t tag) {
    return tag == TAG_STRUCT_BASE ? &info : dream_builtin_type_info(tag);
}
void dream_release_object(dream_ptr p) {
    if (p && dream_rc_last(p)) {
        dream_weak_prepare_destroy(p);
        const dream_type_info *metadata = dream_object_info(p);
        if (metadata->finalize) { metadata->finalize(p); }
        if (metadata->clear) { metadata->clear(p); }
        metadata->reclaim(p);
    }
}
static dream_ptr node(int id) {
    dream_ptr p = dream_malloc(sizeof(Node), TAG_STRUCT_BASE);
    memset(dream_p(p), 0, sizeof(Node));
    ((Node *)dream_p(p))->id = id;
    return p;
}
static void edge(dream_ptr owner, dream_ptr target) {
    dream_retain(target);
    Node *n = dream_p(owner);
    dream_ptr old = n->edge;
    n->edge = target;
    dream_release_object(old);
}
int main(void) {
    int64_t before = debug_get_live_objects();
    dream_ptr parent = node(1), child = node(2);
    edge(parent, child);
    dream_release_object(child);
    assert(!count);
    dream_release_object(parent);
    assert(count == 2 && trace[0] == 1 && trace[1] == 2);
    assert(debug_get_live_objects() == before);

    count = 0;
    dream_ptr self = node(3);
    edge(self, self);
    dream_release_object(self);
    assert(!count && debug_get_live_objects() == before + 1);
    // The internal reference keeps storage alive until explicit cycle teardown.
    edge(self, 0);
    assert(count == 1 && trace[0] == 3 && debug_get_live_objects() == before);

    count = 0;
    dream_ptr a = node(4), b = node(5);
    edge(a, b); edge(b, a);
    dream_release_object(a); dream_release_object(b);
    assert(!count && debug_get_live_objects() == before + 2);
    edge(a, 0);
    assert(count == 2 && trace[0] == 5 && trace[1] == 4);
    assert(debug_get_live_objects() == before);
    puts("plain ARC ownership passed");
    return 0;
}
