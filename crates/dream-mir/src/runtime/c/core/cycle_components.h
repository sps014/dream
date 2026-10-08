#ifndef DREAM_CYCLE_COMPONENTS_H
#define DREAM_CYCLE_COMPONENTS_H

/* All metadata operations hold the cycle gate. Parent links own references, so a
 * representative survives reclamation of the object that originally created it. */
typedef struct CycleComponent {
    struct CycleComponent *parent;
    uint64_t epoch;
    uint32_t references;
    uint16_t rank;
    uint16_t uncertain;
} CycleComponent;

static CycleComponent *free_components;
static uint64_t component_epoch;

static __attribute__((noinline, cold)) void component_grow(void) {
    CycleComponent *chunk = dream_raw_calloc(2048, sizeof(*chunk));
    if (!chunk) { DREAM_PANIC_LITERAL(u"panic: out of memory tracking ownership components"); }
    for (size_t i = 2048; i-- > 0;) {
        chunk[i].parent = free_components;
        free_components = &chunk[i];
    }
}

static CycleComponent *component_take(int uncertain) {
    if (!free_components) { component_grow(); }
    CycleComponent *c = free_components;
    free_components = c->parent;
    *c = (CycleComponent){ .references = 1, .epoch = component_epoch, .uncertain = uncertain };
    return c;
}

static void component_drop(CycleComponent *c) {
    while (c && --c->references == 0) {
        CycleComponent *parent = c->parent;
        c->parent = free_components;
        free_components = c;
        c = parent;
    }
}

static CycleComponent *component_root(CycleComponent *c) {
    while (c->parent) { c = c->parent; }
    return c;
}

static int component_possible(CycleComponent *c) {
    c = component_root(c);
    return c->uncertain || c->epoch != component_epoch;
}

static void component_join(CycleComponent *a, CycleComponent *b) {
    a = component_root(a);
    b = component_root(b);
    if (a == b) { a->uncertain = 1; return; }
    int uncertain = component_possible(a) || component_possible(b);
    if (a->rank < b->rank) { CycleComponent *swap = a; a = b; b = swap; }
    if (a->rank == b->rank) { ++a->rank; }
    b->parent = a;
    if (a->references == UINT32_MAX) { DREAM_PANIC_LITERAL(u"panic: ownership component reference overflow"); }
    ++a->references;
    a->uncertain = uncertain;
    a->epoch = component_epoch;
}

static void component_invalidate_all(void) {
    if (component_epoch == UINT64_MAX) { DREAM_PANIC_LITERAL(u"panic: ownership component epoch exhausted"); }
    ++component_epoch;
    dream_count(DREAM_COUNT_UNKNOWN_STORES, 1);
}

#endif
