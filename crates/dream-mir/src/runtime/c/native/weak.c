#ifdef DREAM_WASM32
#include "../wasm32/include/dream_rt_wasm32.h"
#else
#include "include/dream_rt_native.h"

#include <pthread.h>
#endif

/* Weak / unowned / `Weak<T>` registrations, indexed by target. A target carries
 * `DREAM_TAG_WEAK_TARGET` from its first registration on, so frees of every other object
 * skip the registry entirely and a dying target only visits its own bucket. Nodes stay on
 * the Dream heap (tag 0) so `Debug.live_objects` counts a live registration as before. */
typedef struct dream_weak_node {
    dream_ptr target;
    dream_ptr slot;
    dream_ptr extra;
    int32_t kind;
    struct dream_weak_node *next;
} dream_weak_node;

#define WEAK_BUCKETS 4096u

static dream_weak_node *weak_buckets[WEAK_BUCKETS];

#ifdef DREAM_WASM32
#ifdef DREAM_WASM32_THREADS
static void weak_lock(void) {
    for (;;) {
        int32_t expected = 0;
        if (__atomic_compare_exchange_n(dream_wasm32_meta_i32(DREAM_META_WEAK_LOCK), &expected, 1, 0,
                                        __ATOMIC_ACQUIRE, __ATOMIC_RELAXED)) {
            return;
        }
    }
}

static void weak_unlock(void) {
    __atomic_store_n(dream_wasm32_meta_i32(DREAM_META_WEAK_LOCK), 0, __ATOMIC_RELEASE);
}
#else
static void weak_lock(void) {}
static void weak_unlock(void) {}
#endif
#else
static pthread_mutex_t weak_mu = PTHREAD_MUTEX_INITIALIZER;

static void weak_lock(void) {
    pthread_mutex_lock(&weak_mu);
}

static void weak_unlock(void) {
    pthread_mutex_unlock(&weak_mu);
}
#endif

static dream_weak_node **weak_bucket(dream_ptr target) {
    uint64_t h = ((uint64_t)(uintptr_t)target >> 3) * 0x9E3779B97F4A7C15ull;
    return &weak_buckets[(uint32_t)(h >> 40) & (WEAK_BUCKETS - 1u)];
}

static void weak_free_list(dream_weak_node *dead) {
    while (dead) {
        dream_weak_node *node = dead;
        dead = node->next;
        dream_free((dream_ptr)(uintptr_t)node);
    }
}

void dream_weak_register(dream_ptr target, dream_ptr slot, int32_t kind, dream_ptr extra) {
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
    node->extra = extra;
    node->kind = kind;
    weak_lock();
    __atomic_fetch_or(dream_tag_word(target), DREAM_TAG_WEAK_TARGET, __ATOMIC_RELAXED);
    head = weak_bucket(target);
    node->next = *head;
    *head = node;
    weak_unlock();
}

void dream_weak_unregister(dream_ptr target, dream_ptr slot) {
    dream_weak_node **link;
    if (!target) {
        return;
    }
    weak_lock();
    link = weak_bucket(target);
    while (*link) {
        dream_weak_node *node = *link;
        if (node->target == target && node->slot == slot) {
            *link = node->next;
            weak_unlock();
            dream_free((dream_ptr)(uintptr_t)node);
            return;
        }
        link = &node->next;
    }
    weak_unlock();
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
                *(dream_ptr *)dream_p(node->slot) = node->extra;
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

/* --- Weak-handle slots (`Weak` stdlib class) --------------------------------- */

/* Allocates the registered slot-box for a fresh weak handle holding `value`. The box holds a
 * single raw pointer; when `value` dies, clear_all writes 0 into it (kind 2). */
int64_t weakBind(dream_ptr value) {
    dream_ptr box;
    if (!value) {
        return 0;
    }
    box = dream_malloc((int32_t)sizeof(dream_ptr), 0);
    *(dream_ptr *)dream_p(box) = value;
    dream_weak_register(value, box, 2, 0);
    return (int64_t)(uintptr_t)box;
}

/* Loads the tracked object: NULL when dead, otherwise the payload with its refcount bumped
 * so the caller owns a reference. */
dream_ptr weakLoad(int64_t slot) {
    dream_ptr box = (dream_ptr)(uintptr_t)slot;
    dream_ptr v;
    if (!box) {
        return 0;
    }
    v = *(dream_ptr *)dream_p(box);
    if (!v) {
        return 0;
    }
    dream_retain(v);
    return v;
}

int32_t weakDead(int64_t slot) {
    dream_ptr box = (dream_ptr)(uintptr_t)slot;
    if (!box) {
        return 1;
    }
    return *(dream_ptr *)dream_p(box) == 0;
}

/* Unregisters early (handle dropped before its target) and frees the slot-box: the box
 * outlives a target-death (it holds the dead marker) but dies with the handle. A dead box
 * holds 0, which clear_all already unregistered. */
void weakReleaseRaw(int64_t slot) {
    dream_ptr box = (dream_ptr)(uintptr_t)slot;
    if (!box) {
        return;
    }
    dream_weak_unregister(*(dream_ptr *)dream_p(box), box);
    dream_free(box);
}
