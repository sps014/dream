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
static const dream_type_info info = { visit, finalize, clear, dream_recycle, TAG_STRUCT_BASE };
#define ARRAY_INFO_TAG (TAG_ARRAY_KIND | (TAG_FLOAT16 + 1))
#define FUTURE_INFO_TAG (TAG_FUTURE_KIND | (TAG_FLOAT16 + 2))
static const dream_type_info array_info = {NULL, NULL, NULL, dream_recycle, ARRAY_INFO_TAG};
static const dream_type_info future_info = {NULL, NULL, NULL, dream_recycle, FUTURE_INFO_TAG};
const dream_type_info *dream_type_info_for_tag(int32_t tag) {
    switch (tag) {
    case TAG_STRUCT_BASE: return &info;
    case ARRAY_INFO_TAG: return &array_info;
    case FUTURE_INFO_TAG: return &future_info;
    default: return dream_builtin_type_info(tag);
    }
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

    dream_ptr typed = dream_malloc(8, TAG_ARRAY);
    dream_set_type(typed, &array_info);
    assert(dream_object_tag(typed) == ARRAY_INFO_TAG);
    assert(dream_object_kind(typed) == TAG_ARRAY);
    assert(dream_object_info(typed) == &array_info);
    dream_publish(typed);
    assert(dream_tag_shared(typed));
    assert(dream_object_info(typed) == &array_info);
    dream_release(typed);

    typed = dream_malloc_shared(8, 0);
    dream_set_type(typed, &future_info);
    assert(dream_object_tag(typed) == FUTURE_INFO_TAG);
    assert(dream_object_kind(typed) == TAG_FUTURE);
    assert(dream_tag_shared(typed));
    assert(dream_object_info(typed) == &future_info);
    dream_recycle(typed);
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
