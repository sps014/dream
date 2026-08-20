#include "dream_rt_wasm32.h"

int dream_weak_any;

void dream_weak_register(dream_ptr target, dream_ptr slot, int32_t kind, dream_ptr extra) {
    (void)target;
    (void)slot;
    (void)kind;
    (void)extra;
}

void dream_weak_unregister(dream_ptr target, dream_ptr slot) {
    (void)target;
    (void)slot;
}

void dream_weak_clear_all(dream_ptr obj) { (void)obj; }
