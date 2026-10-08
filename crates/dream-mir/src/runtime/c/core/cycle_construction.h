#ifndef DREAM_CYCLE_CONSTRUCTION_H
#define DREAM_CYCLE_CONSTRUCTION_H

/* Only compiler-verified fresh builders may use this scope. The metadata owner remains
 * live through intermediate zero-count cleanup and is relinquished before publication. */
int dream_cycle_graph_begin(int private_graph) {
    int token = dream_cycle_construction_begin(private_graph);
    if (!token || context()->construction_component) { return token; }
    context()->construction_component = component_take(0);
    return token | 4;
}

void dream_cycle_graph_end(int token) {
    if (token & 4) {
        CycleContext *c = context();
        CycleComponent *component = c->construction_component;
        c->construction_component = NULL;
        component_drop(component);
    }
    dream_cycle_store_end(token & 3);
}

static CycleComponent *construction_component(CycleContext *c, int dynamic) {
    CycleComponent *component = c->construction_component;
    if (!component) { return dynamic ? component_take(1) : NULL; }
    if (component->references == UINT32_MAX) {
        DREAM_PANIC_LITERAL(u"panic: ownership component reference overflow");
    }
    ++component->references;
    if (dynamic) { component_root(component)->uncertain = 1; }
    return component;
}

#endif
