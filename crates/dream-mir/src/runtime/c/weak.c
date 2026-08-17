#include "dream_rt.h"

IMPORT("malloc") int32_t rt_malloc(int32_t size, int32_t tag);
IMPORT("free") void rt_free(int32_t ptr);

EXPORT("weak_register")
void weak_register(int32_t target, int32_t slot, int32_t kind, int32_t extra) {
    int32_t node;
    if (target == 0) {
        return;
    }
    node = rt_malloc(20, 0);
    i32_store(node, target);
    i32_store(node + 4, slot);
    i32_store(node + 8, kind);
    i32_store(node + 12, extra);
    i32_store(node + 16, weak_list_head);
    weak_list_head = node;
}

EXPORT("weak_unregister")
void weak_unregister(int32_t target, int32_t slot) {
    int32_t prev = 0;
    int32_t curr;
    int32_t next;
    if (target == 0) {
        return;
    }
    curr = weak_list_head;
    while (curr != 0) {
        next = i32_load(curr + 16);
        if (i32_load(curr) != target || i32_load(curr + 4) != slot) {
            prev = curr;
            curr = next;
            continue;
        }
        if (prev == 0) {
            weak_list_head = next;
        } else {
            i32_store(prev + 16, next);
        }
        rt_free(curr);
        return;
    }
}

EXPORT("weak_clear_all")
void weak_clear_all(int32_t target) {
    int32_t prev = 0;
    int32_t curr;
    int32_t next;
    int32_t slot;
    int32_t kind;
    if (target == 0) {
        return;
    }
    curr = weak_list_head;
    while (curr != 0) {
        next = i32_load(curr + 16);
        if (i32_load(curr) == target) {
            slot = i32_load(curr + 4);
            kind = i32_load(curr + 8);
            if (kind == 0) {
                i32_store(slot, i32_load(curr + 12));
                i32_store(slot + 4, 0);
            } else {
                i32_store(slot, 0);
            }
            if (prev == 0) {
                weak_list_head = next;
            } else {
                i32_store(prev + 16, next);
            }
            rt_free(curr);
        } else {
            prev = curr;
        }
        curr = next;
    }
}
