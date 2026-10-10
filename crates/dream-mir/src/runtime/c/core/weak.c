#include "dream_core.h"
#include "dream_platform_internal.h"

/* Weak / unowned / `Weak<T>` registrations, indexed by target. A target carries
 * `DREAM_TAG_WEAK_TARGET` while registrations exist, so frees of every other object
 * skip the registry entirely and a dying target only visits its own bucket. Registration
 * records have no ARC edges or destructors, so pooled metadata needs no object header. */
typedef struct dream_weak_node {
    dream_ptr target;
    dream_ptr slot;
    int32_t none_tag;
    int32_t kind;
    struct dream_weak_node *next;
} dream_weak_node;

#define WEAK_BUCKETS 4096u

static dream_weak_node *weak_buckets[WEAK_BUCKETS];
static dream_weak_node *free_nodes;

/* Slot snapshots and observed zero claims share the registry lock; user finalizers
 * always run after the claim has released it. Nesting is confined to registry helpers. */
#ifndef DREAM_WASM32
static _Thread_local unsigned weak_depth;
static unsigned depth_get(void) { return weak_depth; }
static void depth_set(unsigned depth) { weak_depth = depth; }
#else
extern int32_t dream_weak_depth_get(void);
extern void dream_weak_depth_set(int32_t);
static unsigned depth_get(void) { return (unsigned)dream_weak_depth_get(); }
static void depth_set(unsigned depth) { dream_weak_depth_set((int32_t)depth); }
#endif
void dream_weak_enter(void) {
    unsigned depth = depth_get();
    if (depth == 0) { dream_platform_current->lock(DREAM_LOCK_WEAK); }
    depth_set(depth + 1);
}
void dream_weak_leave(void) {
    unsigned depth = depth_get();
    if (depth == 0) { DREAM_PANIC_LITERAL(u"panic: weak registry lock underflow"); }
    depth_set(depth - 1);
    if (depth == 1) { dream_platform_current->unlock(DREAM_LOCK_WEAK); }
}
static int weak_lock(void) { dream_weak_enter(); return 1; }
static void weak_unlock(int acquired) { if (acquired) { dream_weak_leave(); } }

__attribute__((cold, noinline)) int dream_rc_last_observed(dream_ptr ptr) {
    dream_weak_enter();
    int last = dream_rc_last_raw(ptr);
    dream_weak_leave();
    return last;
}
__attribute__((cold, noinline)) int dream_rc_claim_observed(dream_ptr ptr) {
    dream_weak_enter();
    int32_t expected = __atomic_load_n(dream_rc_word(ptr), __ATOMIC_RELAXED);
    int last = (expected & INT32_MAX) == 1 && __atomic_compare_exchange_n(
        dream_rc_word(ptr), &expected, 0, 0, __ATOMIC_ACQ_REL, __ATOMIC_RELAXED);
    dream_weak_leave();
    return last;
}

static dream_weak_node *weak_take_locked(void) {
    if (!free_nodes) {
        dream_weak_node *chunk = dream_raw_calloc(1024, sizeof(*chunk));
        if (!chunk) { DREAM_PANIC_LITERAL(u"panic: out of memory registering weak references"); }
        for (size_t i = 1024; i-- > 0;) {
            chunk[i].next = free_nodes;
            free_nodes = &chunk[i];
        }
    }
    dream_weak_node *node = free_nodes;
    free_nodes = node->next;
    uint64_t allocations = __atomic_load_n(&dream_weak_allocations, __ATOMIC_RELAXED);
    __atomic_store_n(&dream_weak_allocations, allocations + 1, __ATOMIC_RELAXED);
    return node;
}

static void weak_give_locked(dream_weak_node *node) {
    node->next = free_nodes;
    free_nodes = node;
    uint64_t frees = __atomic_load_n(&dream_weak_frees, __ATOMIC_RELAXED);
    __atomic_store_n(&dream_weak_frees, frees + 1, __ATOMIC_RELAXED);
}

static dream_weak_node **weak_bucket(dream_ptr target) {
    uint64_t h = ((uint64_t)(uintptr_t)target >> 3) * 0x9E3779B97F4A7C15ull;
    return &weak_buckets[(uint32_t)(h >> 40) & (WEAK_BUCKETS - 1u)];
}

static void weak_free_list(dream_weak_node *dead) {
    while (dead) {
        dream_weak_node *node = dead;
        dead = node->next;
        weak_give_locked(node);
    }
}

void dream_weak_register(dream_ptr target, dream_ptr slot, int32_t kind, int32_t none_tag) {
    dream_weak_node *node;
    dream_weak_node **head;
    if (!target || !slot) {
        return;
    }
    int acquired = weak_lock();
    if (__atomic_load_n(dream_rc_word(target), __ATOMIC_RELAXED) == 0) {
        DREAM_PANIC_LITERAL(u"panic: weak registration of a dying object");
    }
    /* Observation needs an atomic target count, not publication of its owned graph.
     * A successful load publishes that graph before exposing its temporary owner. */
    __atomic_fetch_or(dream_rc_word(target), DREAM_RC_SHARED_BIT, __ATOMIC_RELAXED);
    node = weak_take_locked();
    node->target = target;
    node->slot = slot;
    node->none_tag = none_tag;
    node->kind = kind;
    __atomic_fetch_or(dream_tag_word(target), DREAM_TAG_WEAK_TARGET, __ATOMIC_RELAXED);
    dream_count(DREAM_COUNT_WEAK_REGISTER, 1);
    head = weak_bucket(target);
    node->next = *head;
    *head = node;
    weak_unlock(acquired);
}

static dream_weak_node *weak_remove_locked(dream_ptr target, dream_ptr slot) {
    dream_weak_node **link;
    if (!target) {
        return NULL;
    }
    link = weak_bucket(target);
    while (*link) {
        dream_weak_node *node = *link;
        if (node->target == target && node->slot == slot) {
            *link = node->next;
            dream_weak_node *remaining = *weak_bucket(target);
            while (remaining && remaining->target != target) { remaining = remaining->next; }
            if (!remaining) {
                __atomic_fetch_and(dream_tag_word(target), ~DREAM_TAG_WEAK_TARGET, __ATOMIC_RELAXED);
            }
            return node;
        }
        link = &node->next;
    }
    return NULL;
}

void dream_weak_unregister(dream_ptr target, dream_ptr slot) {
    int acquired = weak_lock();
    dream_weak_node *node = weak_remove_locked(target, slot);
    if (node != NULL) {
        weak_give_locked(node);
    }
    weak_unlock(acquired);
}

void dream_weak_clear_all(dream_ptr obj) {
    dream_weak_node *dead = NULL;
    dream_weak_node **link;
    int acquired = weak_lock();
    link = weak_bucket(obj);
    while (*link) {
        dream_weak_node *node = *link;
        if (node->target == obj) {
            if (node->kind == 2) {
                /* Weak handle: target died — mark the slot dead (null payload). */
                *(dream_ptr *)dream_p(node->slot) = 0;
            } else if (node->kind == 0) {
                *(int32_t *)dream_p(node->slot) = node->none_tag;
                *(dream_ptr *)((char *)dream_p(node->slot) + sizeof(dream_ptr)) = 0;
            } else {
                /* unowned: poison so a later load reports "target destroyed" rather
                 * than an ambiguous null deref. */
                *(dream_ptr *)dream_p(node->slot) = (dream_ptr)(intptr_t)DREAM_UNOWNED_POISON;
            }
            *link = node->next;
            node->next = dead;
            dead = node;
            dream_count(DREAM_COUNT_WEAK_INVALIDATE, 1);
        } else {
            link = &node->next;
        }
    }
    __atomic_fetch_and(dream_tag_word(obj), ~DREAM_TAG_WEAK_TARGET, __ATOMIC_RELAXED);
    weak_free_list(dead);
    weak_unlock(acquired);
}

void dream_weak_prepare_destroy(dream_ptr ptr) {
    if (__atomic_load_n(dream_tag_word(ptr), __ATOMIC_RELAXED) & DREAM_TAG_WEAK_TARGET) {
        dream_weak_clear_all(ptr);
    }
}

/* --- Weak-handle slots (`Weak` stdlib class) --------------------------------- */

typedef struct {
    dream_ptr value;
    int32_t immortal;
} WeakBox;

/* Allocates the registered slot-box for a fresh weak handle holding `value`. The box holds a
 * single raw pointer; when `value` dies, clear_all writes 0 into it (kind 2). */
uintptr_t weakBind(dream_ptr value) {
    dream_ptr box;
    if (!value) {
        return 0;
    }
    int32_t immortal = __atomic_load_n(dream_rc_word(value), __ATOMIC_RELAXED) == DREAM_RC_IMMORTAL;
    /* The opaque slot does not expose its target to Task's graph walker. */
    box = dream_malloc((int32_t)sizeof(WeakBox), 0);
    WeakBox *data = (WeakBox *)dream_p(box);
    data->value = value;
    data->immortal = immortal;
    dream_weak_register(value, box, 2, 0);
    return (uintptr_t)box;
}

/* Loads the tracked object: NULL when dead, otherwise the payload with its refcount bumped
 * so the caller owns a reference. */
dream_ptr weakLoad(uintptr_t slot) {
    dream_ptr box = (dream_ptr)slot;
    dream_ptr v;
    if (!box) {
        return 0;
    }
    int acquired = weak_lock();
    WeakBox *data = (WeakBox *)dream_p(box);
    v = data->value;
    if (!v) {
        weak_unlock(acquired);
        return 0;
    }
    if (data->immortal) {
        weak_unlock(acquired);
        return v;
    }
    int32_t *rc = dream_rc_word(v);
    int32_t count = __atomic_load_n(rc, __ATOMIC_RELAXED);
    if (count == DREAM_RC_IMMORTAL) {
        data->immortal = 1;
        weak_unlock(acquired);
        return v;
    }
    while ((count & INT32_MAX) != 0) {
        if ((count & INT32_MAX) == INT32_MAX) {
            weak_unlock(acquired);
            dream_panic(dream_utf8_to_string("reference count overflow loading a weak target"));
            return 0;
        }
        int32_t next = (int32_t)((uint32_t)count + 1u);
        if (__atomic_compare_exchange_n(rc, &count, next, 0, __ATOMIC_ACQUIRE, __ATOMIC_RELAXED)) {
            weak_unlock(acquired);
            dream_publish(v);
            return v;
        }
    }
    weak_unlock(acquired);
    return 0;
}

int32_t weakDead(uintptr_t slot) {
    dream_ptr box = (dream_ptr)slot;
    if (!box) {
        return 1;
    }
    int acquired = weak_lock();
    WeakBox *data = (WeakBox *)dream_p(box);
    dream_ptr value = data->value;
    int32_t count = value ? __atomic_load_n(dream_rc_word(value), __ATOMIC_RELAXED) : 0;
    int32_t dead = value == 0 ||
        (!data->immortal && count != DREAM_RC_IMMORTAL && (count & INT32_MAX) == 0);
    weak_unlock(acquired);
    return dead;
}

/* Unregisters early (handle dropped before its target) and frees the slot-box: the box
 * outlives a target-death (it holds the dead marker) but dies with the handle. A dead box
 * holds 0, which clear_all already unregistered. */
void weakReleaseRaw(uintptr_t slot) {
    dream_ptr box = (dream_ptr)slot;
    if (!box) {
        return;
    }
    int acquired = weak_lock();
    WeakBox *data = (WeakBox *)dream_p(box);
    dream_weak_node *node = weak_remove_locked(data->value, box);
    data->value = 0;
    if (node != NULL) {
        weak_give_locked(node);
    }
    weak_unlock(acquired);
    dream_free(box);
}

/* Non-niche Option fields own their slot box, but its payload is non-owning. A read
 * returns a separate strong snapshot so target invalidation cannot mutate that value. */
dream_ptr dream_weak_load_field(dream_ptr slot, int32_t kind, int32_t none_tag,
                                int32_t payload_offset, dream_size size, int32_t tag,
                                const dream_type_info *info) {
    dream_weak_enter();
    dream_ptr stored;
    memcpy(&stored, dream_p(slot), sizeof(stored));
    dream_ptr target = stored;
    dream_ptr result = 0;
    if (kind == 0) {
        result = dream_malloc(size, tag);
        dream_set_type(result, info);
        if (stored) {
            memcpy(dream_p(result), dream_p(stored), (size_t)size);
            int32_t discriminant;
            memcpy(&discriminant, dream_p(stored), sizeof(discriminant));
            if (discriminant == none_tag) { target = 0; }
            else { memcpy(&target, (char *)dream_p(stored) + payload_offset, sizeof(target)); }
        } else {
            memset(dream_p(result), 0, (size_t)size);
            memcpy(dream_p(result), &none_tag, sizeof(none_tag));
            target = 0;
        }
    }
    int unset = kind == 1 && target == 0;
    int dead = kind == 1 && target == (dream_ptr)(intptr_t)DREAM_UNOWNED_POISON;
    if (!unset && !dead && target) {
        if (dream_rc_count(target) == 0) { target = 0; dead = kind == 1; }
        else { dream_retain(target); }
    }
    if (kind == 0 && !target) {
        memcpy(dream_p(result), &none_tag, sizeof(none_tag));
        dream_ptr zero = 0;
        memcpy((char *)dream_p(result) + payload_offset, &zero, sizeof(zero));
    }
    dream_weak_leave();
    if (unset) { DREAM_PANIC_LITERAL(u"panic: read of unset 'unowned' field (referent not assigned, or already deallocated)"); }
    if (dead) { DREAM_PANIC_LITERAL(u"panic: 'unowned' reference used after its target was destroyed"); }
    if (target) { dream_publish(target); }
    return kind == 0 ? result : target;
}

/* Invalidation may race holder teardown, so slot reads and deregistration share
 * the same claim lock. The non-owning Option box is freed after leaving it. */
void dream_weak_drop_field(dream_ptr slot, int32_t kind, int32_t none_tag, int32_t payload_offset) {
    dream_weak_enter();
    dream_ptr stored;
    memcpy(&stored, dream_p(slot), sizeof(stored));
    dream_ptr target = stored;
    dream_ptr registered_slot = slot;
    if (kind == 0 && stored) {
        int32_t discriminant;
        memcpy(&discriminant, dream_p(stored), sizeof(discriminant));
        target = 0;
        if (discriminant != none_tag) {
            memcpy(&target, (char *)dream_p(stored) + payload_offset, sizeof(target));
        }
        registered_slot = stored;
    }
    dream_weak_node *node = weak_remove_locked(target, registered_slot);
    if (node) { weak_give_locked(node); }
    dream_ptr zero = 0;
    memcpy(dream_p(slot), &zero, sizeof(zero));
    dream_weak_leave();
    if (kind == 0 && stored) { dream_free(stored); }
}
