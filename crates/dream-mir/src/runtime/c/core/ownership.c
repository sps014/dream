#include "dream_core.h"

typedef struct VisitContext {
    void (*edge)(dream_ptr, void *);
    void *argument;
} VisitContext;
#ifndef DREAM_WASM32
static _Thread_local VisitContext *visit_context;
static VisitContext *visit_get(void) { return visit_context; }
static void visit_set(VisitContext *c) { visit_context = c; }
#else
extern dream_ptr dream_visit_context_get(void);
extern void dream_visit_context_set(dream_ptr);
static VisitContext *visit_get(void) { return (VisitContext *)dream_p(dream_visit_context_get()); }
static void visit_set(VisitContext *c) { dream_visit_context_set((dream_ptr)c); }
#endif

void dream_visit_edge(dream_ptr p) {
    VisitContext *c = visit_get();
    if (p && c) { c->edge(p, c->argument); }
}
void dream_visit_owned(dream_ptr p, void (*edge)(dream_ptr, void *), void *argument) {
    const dream_type_info *info = dream_object_info(p);
    if (!info || !info->visit) { return; }
    VisitContext current = {edge, argument};
    VisitContext *saved = visit_get();
    visit_set(&current);
    info->visit(p);
    visit_set(saved);
}
uint64_t dream_runtime_counters[DREAM_COUNT_LIMIT];
int64_t debug_get_runtime_counter(int32_t id) {
    if (id < 0 || id >= DREAM_COUNT_LIMIT) { return 0; }
    return (int64_t)__atomic_load_n(&dream_runtime_counters[id], __ATOMIC_RELAXED);
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
static const dream_type_info string_info = { string_visit, NULL, NULL, NULL};
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
static const dream_type_info closure_info = { closure_visit, NULL, closure_clear, dream_recycle};
static const dream_type_info environment_info = { environment_visit, NULL, environment_clear, dream_recycle};
const dream_type_info *dream_builtin_type_info(int32_t tag) {
    switch (tag) {
    case TAG_STRING: return &string_info;
    case TAG_FUNCBOX: return &closure_info;
    case TAG_CLOSURE_ENV: return &environment_info;
    default: return NULL;
    }
}
