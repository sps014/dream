#ifndef DREAM_OWNERSHIP_H
#define DREAM_OWNERSHIP_H

typedef struct dream_type_info {
    void (*visit)(dream_ptr);
    void (*finalize)(dream_ptr);
    void (*clear)(dream_ptr);
    void (*reclaim)(dream_ptr);
    int32_t cycle_capable;
} dream_type_info;

const dream_type_info *dream_builtin_type_info(int32_t tag);
const dream_type_info *dream_type_info_for_tag(int32_t tag);
void dream_set_type(dream_ptr ptr, const dream_type_info *info);
void dream_visit_owned(dream_ptr ptr, void (*edge)(dream_ptr, void *), void *context);
void dream_visit_edge(dream_ptr ptr);
int dream_cycle_store_begin(dream_ptr owner, dream_ptr child, int value);
void dream_cycle_store_end(int locked);
void dream_cycle_enter(void);
void dream_cycle_leave(void);
int dream_cycle_release(dream_ptr ptr);
void dream_cycle_retain(dream_ptr ptr);
void dream_cycle_forget(dream_ptr ptr);
void dream_cycle_check_store(dream_ptr owner, dream_ptr child);
void dream_cycle_drain(void);
void dream_cycle_finish(void);
int dream_cycle_defer_destroy(dream_ptr ptr, void (*destroy)(dream_ptr));
void dream_cycle_postpone(int value);

DREAM_ALWAYS_INLINE const dream_type_info *dream_object_info(dream_ptr p) {
    const dream_type_info *info;
    memcpy(&info, (char *)dream_p(p) - DREAM_BLOCK_HEADER + sizeof(dream_size), sizeof(info));
    return info;
}
#endif
