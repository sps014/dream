#include "include/dream_rt_native.h"

#include <pthread.h>
#include <stdlib.h>

typedef struct dream_weak_node {
    dream_ptr target;
    dream_ptr slot;
    dream_ptr extra;
    int32_t kind;
    struct dream_weak_node *next;
} dream_weak_node;

static dream_weak_node *weak_list_head;
static pthread_mutex_t weak_mu = PTHREAD_MUTEX_INITIALIZER;
int dream_weak_any;

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
    pthread_mutex_lock(&weak_mu);
    node->next = weak_list_head;
    weak_list_head = node;
    dream_weak_any = 1;
    pthread_mutex_unlock(&weak_mu);
}

void dream_weak_unregister(dream_ptr target, dream_ptr slot) {
    dream_weak_node **link;
    pthread_mutex_lock(&weak_mu);
    link = &weak_list_head;
    while (*link) {
        dream_weak_node *node = *link;
        if (node->target == target && node->slot == slot) {
            *link = node->next;
            dream_weak_any = weak_list_head != NULL;
            pthread_mutex_unlock(&weak_mu);
            dream_free((dream_ptr)(uintptr_t)node);
            return;
        }
        link = &node->next;
    }
    pthread_mutex_unlock(&weak_mu);
}

/* Removes every registration whose slot-box is `slot` (weak handle dropped early). */
void dream_weak_unregister_by_slot(dream_ptr slot_box) {
    if (!slot_box || !dream_weak_any) {
        return;
    }
    dream_weak_node *dead = NULL;
    pthread_mutex_lock(&weak_mu);
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
    pthread_mutex_unlock(&weak_mu);
    while (dead) {
        dream_weak_node *node = dead;
        dead = node->next;
        dream_free((dream_ptr)(uintptr_t)node);
    }
}

void dream_weak_clear_all(dream_ptr obj) {
    dream_weak_node *dead = NULL;
    if (weak_list_head == NULL) {
        return;
    }
    pthread_mutex_lock(&weak_mu);
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
    pthread_mutex_unlock(&weak_mu);
    dream_weak_any = weak_list_head != NULL;
    while (dead) {
        dream_weak_node *node = dead;
        dead = node->next;
        dream_free((dream_ptr)(uintptr_t)node);
    }
}


/* --- Weak-handle slots (`Weak` stdlib class) --------------------------------- */

/* Allocates the registered slot-box for a fresh weak handle holding `value`. The box holds a
 * single raw pointer; when `value` dies, clear_all writes 0 into it (kind 2). */
int64_t weakBind(dream_ptr value) {
    if (!value) {
        return 0;
    }
    dream_ptr box = dream_malloc((int32_t)sizeof(dream_ptr), 0);
    *(dream_ptr *)dream_p(box) = value;
    dream_weak_register(value, box, 2, 0);
    return (int64_t)(uintptr_t)box;
}

/* Loads the tracked object: NULL when dead, otherwise the payload with its refcount bumped
 * so the caller owns a reference (header rc sits at user_ptr - 4 in both runtimes). */
dream_ptr weakLoad(int64_t slot) {
    dream_ptr box = (dream_ptr)(uintptr_t)slot;
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
    dream_ptr box = (dream_ptr)(uintptr_t)slot;
    if (!box) {
        return 1;
    }
    return *(dream_ptr *)dream_p(box) == 0;
}

/* Unregisters early (handle dropped before its target) and frees the slot-box: the box
 * outlives a target-death (it holds the dead marker) but dies with the handle. */
void weakReleaseRaw(int64_t slot) {
    dream_ptr box = (dream_ptr)(uintptr_t)slot;
    if (!box) {
        return;
    }
    dream_weak_unregister_by_slot(box);
    dream_free(box);
}
