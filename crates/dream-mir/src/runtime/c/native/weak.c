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
    pthread_mutex_unlock(&weak_mu);
    dream_weak_any = weak_list_head != NULL;
    while (dead) {
        dream_weak_node *node = dead;
        dead = node->next;
        dream_free((dream_ptr)(uintptr_t)node);
    }
}
