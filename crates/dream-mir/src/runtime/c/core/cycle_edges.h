#ifndef DREAM_CYCLE_EDGES_H
#define DREAM_CYCLE_EDGES_H

static void check_store_locked(dream_ptr owner, dream_ptr child, int strong) {
    CycleNode *a = owner ? find_node(owner) : NULL;
    CycleNode *b = child ? find_node(child) : NULL;
    if ((a && a->dying) || (b && b->dying)) {
        DREAM_PANIC_LITERAL(u"panic: publication or mutation of a dying object");
    }
    if (strong && a && b) {
        if (a == b) {
            if (!a->component) { a->component = component_take(1); }
            else { component_root(a->component)->uncertain = 1; }
            queue_node(a);
            return;
        }
        /* No recorded edge can reach an isolated node without giving it a component.
         * Unknown stores invalidate older isolated allocations by their stable identity. */
        if (!a->component && !b->component) {
            CycleComponent *component = component_take(
                a->sequence <= unknown_store_sequence || b->sequence <= unknown_store_sequence);
            component->references = 2;
            a->component = b->component = component;
            if (component->uncertain) { queue_node(a); }
            return;
        }
        if (!a->component || !b->component) {
            CycleNode *isolated = a->component ? b : a;
            CycleComponent *component = component_root(a->component ? a->component : b->component);
            if (component->references == UINT32_MAX) { DREAM_PANIC_LITERAL(u"panic: ownership component reference overflow"); }
            ++component->references;
            component->uncertain = component_possible(component) || isolated->sequence <= unknown_store_sequence;
            component->epoch = component_epoch;
            isolated->component = component;
            if (component->uncertain) { queue_node(a); }
            return;
        }
        CycleComponent *own = a->component;
        CycleComponent *other = component_root(b->component);
        /* A verified initializer writes a fresh, unpublished owner. Adding its outgoing
         * edges cannot close a path back to that owner, including shared DAG children. */
        if (strong == 2 && component_root(own) == other) { return; }
        if (own != other && !own->parent && own->references == 1) {
            /* An isolated descriptor has no other metadata owner. Share the destination
             * root directly instead of preserving a redundant parent link. */
            int uncertain = component_possible(own) || component_possible(other);
            if (other->references == UINT32_MAX) { DREAM_PANIC_LITERAL(u"panic: ownership component reference overflow"); }
            ++other->references;
            other->uncertain = uncertain;
            other->epoch = component_epoch;
            a->component = other;
            component_drop(own);
        } else { component_join(own, other); }
        /* Ownership transfers may leave no external token to decrement after this store. */
        if (node_possible(a)) { queue_node(a); }
    } else if (strong && b && node_possible(b)) {
        queue_node(b);
    }
}

#endif
