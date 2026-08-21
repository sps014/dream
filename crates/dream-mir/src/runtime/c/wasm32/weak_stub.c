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
                if (node->kind == 0) {
                    *(dream_ptr *)dream_p(node->slot) = node->extra;
                    *(dream_ptr *)((char *)dream_p(node->slot) + sizeof(dream_ptr)) = 0;
                } else {
                    *(dream_ptr *)dream_p(node->slot) = 0;
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
