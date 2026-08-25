/* Weak-reference registry for the wasm32 guest — port of native/weak.c with the pthread
 * mutex replaced by the heap's atomic spin-lock idiom (separate META slot; must not be
 * the allocator lock, since register() allocates its node). */
#include "dream_rt_wasm32.h"

typedef struct dream_weak_node {
    dream_ptr target;
    dream_ptr slot;
    dream_ptr extra;
    int32_t kind;
    struct dream_weak_node *next;
} dream_weak_node;

static dream_weak_node *weak_list_head;
int dream_weak_any;

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

void dream_weak_register(dream_ptr target, dream_ptr slot, int32_t kind, dream_ptr extra) {
    dream_weak_node *node;
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
    node->next = weak_list_head;
    weak_list_head = node;
    dream_weak_any = 1;
    weak_unlock();
}

void dream_weak_unregister(dream_ptr target, dream_ptr slot) {
    dream_weak_node **link;
    weak_lock();
    link = &weak_list_head;
    while (*link) {
        dream_weak_node *node = *link;
        if (node->target == target && node->slot == slot) {
            *link = node->next;
            dream_weak_any = weak_list_head != NULL;
            weak_unlock();
            dream_free((dream_ptr)(uintptr_t)node);
            return;
        }
        link = &node->next;
    }
    weak_unlock();
}

/* Removes every registration whose slot-box is `slot` (weak handle dropped early). */
void dream_weak_unregister_by_slot(dream_ptr slot_box) {
    if (!slot_box || !dream_weak_any) {
        return;
    }
    dream_weak_node *dead = NULL;
    weak_lock();
    {
        dream_weak_node **link = &weak_list_head;
        while (*link) {
            dream_weak_node *node = *link;
            if (node->slot == slot_box) {
                *link = node->next;
                node->next = dead;
                dead = node;
            } else {
                link = &node->next;
            }
        }
        dream_weak_any = weak_list_head != NULL;
    }
    weak_unlock();
    while (dead) {
        dream_weak_node *node = dead;
        dead = node->next;
        dream_free((dream_ptr)(uintptr_t)node);
    }
}

int64_t weakBind(dream_ptr value) {
    if (!value) {
        return 0;
    }
    dream_ptr box = dream_malloc((int32_t)sizeof(dream_ptr), 0);
    *(dream_ptr *)dream_p(box) = value;
    dream_weak_register(value, box, 2, 0);
    return (int64_t)(uintptr_t)box;
}

dream_ptr weakLoad(int64_t slot) {
    dream_ptr box = (dream_ptr)(int32_t)slot;
    if (!box) {
        return 0;
    }
    dream_ptr v = *(dream_ptr *)dream_p(box);
    if (!v) {
        return 0;
    }
    ((int32_t *)((char *)dream_p(v) - 4))[0] += 1;
    return v;
}

int32_t weakDead(int64_t slot) {
    dream_ptr box = (dream_ptr)(int32_t)slot;
    if (!box) {
        return 1;
    }
    return *(dream_ptr *)dream_p(box) == 0;
}

void weakReleaseRaw(int64_t slot) {
    dream_ptr box = (dream_ptr)(int32_t)slot;
    if (!box) {
        return;
    }
    dream_weak_unregister_by_slot(box);
    dream_free(box);
}

void dream_weak_clear_all(dream_ptr obj) {
    dream_weak_node *dead = NULL;
    if (weak_list_head == NULL) {
        return;
    }
    weak_lock();
    {
        dream_weak_node **link = &weak_list_head;
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
    }
    weak_unlock();
    dream_weak_any = weak_list_head != NULL;
    while (dead) {
        dream_weak_node *node = dead;
        dead = node->next;
        dream_free((dream_ptr)(uintptr_t)node);
    }
}


/* --- Weak-handle slots (Weak<T> stdlib class) — mirrors native/weak.c ---------------- */

dream_ptr dream_weak_slot_make(dream_ptr value) {
    if (!value) {
        return 0;
    }
    dream_ptr box = dream_malloc((int32_t)sizeof(dream_ptr), 0);
    *(dream_ptr *)dream_p(box) = value;
    dream_weak_register(value, box, 2, 0);
    return box;
}

dream_ptr dream_weak_slot_load(dream_ptr slot_box) {
    if (!slot_box) {
        return 0;
    }
    dream_ptr v = *(dream_ptr *)dream_p(slot_box);
    if (!v) {
        return 0;
    }
    ((int32_t *)((char *)dream_p(v) - 4))[0] += 1;
    return v;
}

int32_t dream_weak_slot_dead(dream_ptr slot_box) {
    if (!slot_box) {
        return 1;
    }
    return *(dream_ptr *)dream_p(slot_box) == 0;
}

void dream_weak_slot_release(dream_ptr target, dream_ptr slot_box) {
    dream_weak_unregister(target, slot_box);
}
