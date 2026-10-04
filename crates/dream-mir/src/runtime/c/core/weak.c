#include "dream_core.h"
#include "dream_platform_internal.h"

/* Weak / unowned / `Weak<T>` registrations, indexed by target. A target carries
 * `DREAM_TAG_WEAK_TARGET` from its first registration on, so frees of every other object
 * skip the registry entirely and a dying target only visits its own bucket. Nodes stay on
 * the Dream heap (tag 0) so `Debug.live_objects` counts a live registration as before. */
typedef struct dream_weak_node {
    dream_ptr target;
    dream_ptr slot;
    int32_t none_tag;
    int32_t kind;
    struct dream_weak_node *next;
} dream_weak_node;

#define WEAK_BUCKETS 4096u

static dream_weak_node *weak_buckets[WEAK_BUCKETS];

static void weak_lock(void) { dream_platform_current->lock(DREAM_LOCK_WEAK); }
static void weak_unlock(void) { dream_platform_current->unlock(DREAM_LOCK_WEAK); }

static dream_weak_node **weak_bucket(dream_ptr target) {
    uint64_t h = ((uint64_t)(uintptr_t)target >> 3) * 0x9E3779B97F4A7C15ull;
    return &weak_buckets[(uint32_t)(h >> 40) & (WEAK_BUCKETS - 1u)];
}

static void weak_free_list(dream_weak_node *dead) {
    while (dead) {
        dream_weak_node *node = dead;
        dead = node->next;
        dream_free((dream_ptr)node);
    }
}

void dream_weak_register(dream_ptr target, dream_ptr slot, int32_t kind, int32_t none_tag) {
    dream_weak_node *node;
    dream_weak_node **head;
    dream_ptr block;
    if (!target || !slot) {
        return;
    }
    block = dream_malloc((int32_t)sizeof(dream_weak_node), 0);
    node = (dream_weak_node *)dream_p(block);
    node->target = target;
    node->slot = slot;
    node->none_tag = none_tag;
    node->kind = kind;
    weak_lock();
    __atomic_fetch_or(dream_tag_word(target), DREAM_TAG_WEAK_TARGET, __ATOMIC_RELAXED);
    head = weak_bucket(target);
    node->next = *head;
    *head = node;
    weak_unlock();
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
            return node;
        }
        link = &node->next;
    }
    return NULL;
}

void dream_weak_unregister(dream_ptr target, dream_ptr slot) {
    weak_lock();
    dream_weak_node *node = weak_remove_locked(target, slot);
    weak_unlock();
    if (node != NULL) {
        dream_free((dream_ptr)node);
    }
}

void dream_weak_clear_all(dream_ptr obj) {
    dream_weak_node *dead = NULL;
    dream_weak_node **link;
    weak_lock();
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
        } else {
            link = &node->next;
        }
    }
    __atomic_fetch_and(dream_tag_word(obj), ~DREAM_TAG_WEAK_TARGET, __ATOMIC_RELAXED);
    weak_unlock();
    weak_free_list(dead);
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
    dream_publish(value);
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
    weak_lock();
    WeakBox *data = (WeakBox *)dream_p(box);
    v = data->value;
    if (!v) {
        weak_unlock();
        return 0;
    }
    if (data->immortal) {
        weak_unlock();
        return v;
    }
    int32_t *rc = dream_rc_word(v);
    int32_t count = __atomic_load_n(rc, __ATOMIC_RELAXED);
    while ((count & INT32_MAX) != 0) {
        if ((count & INT32_MAX) == INT32_MAX) {
            weak_unlock();
            dream_panic(dream_utf8_to_string("reference count overflow loading a weak target"));
            return 0;
        }
        int32_t next = (int32_t)((uint32_t)count + 1u);
        if (__atomic_compare_exchange_n(rc, &count, next, 0, __ATOMIC_ACQUIRE, __ATOMIC_RELAXED)) {
            weak_unlock();
            return v;
        }
    }
    weak_unlock();
    return 0;
}

int32_t weakDead(uintptr_t slot) {
    dream_ptr box = (dream_ptr)slot;
    if (!box) {
        return 1;
    }
    weak_lock();
    WeakBox *data = (WeakBox *)dream_p(box);
    dream_ptr value = data->value;
    int32_t dead = value == 0 ||
        (!data->immortal && (__atomic_load_n(dream_rc_word(value), __ATOMIC_RELAXED) & INT32_MAX) == 0);
    weak_unlock();
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
    weak_lock();
    WeakBox *data = (WeakBox *)dream_p(box);
    dream_weak_node *node = weak_remove_locked(data->value, box);
    data->value = 0;
    weak_unlock();
    if (node != NULL) {
        dream_free((dream_ptr)node);
    }
    dream_free(box);
}
