#ifndef DREAM_OWNERSHIP_H
#define DREAM_OWNERSHIP_H

typedef struct dream_type_info {
    void (*visit)(dream_ptr);
    void (*finalize)(dream_ptr);
    void (*clear)(dream_ptr);
    void (*reclaim)(dream_ptr);
} dream_type_info;

const dream_type_info *dream_builtin_type_info(int32_t tag);
const dream_type_info *dream_type_info_for_tag(int32_t tag);
void dream_visit_owned(dream_ptr ptr, void (*edge)(dream_ptr, void *), void *context);
void dream_visit_edge(dream_ptr ptr);
void dream_weak_enter(void);
void dream_weak_leave(void);
void dream_weak_drop_field(dream_ptr slot, int32_t kind, int32_t none_tag, int32_t payload_offset);
int dream_rc_last_observed(dream_ptr ptr);
int dream_rc_claim_observed(dream_ptr ptr);
dream_ptr dream_weak_load_field(dream_ptr slot, int32_t kind, int32_t none_tag,
                                int32_t payload_offset, dream_size size, int32_t tag,
                                const dream_type_info *info);
/* Opt-in ownership attribution. Instrumentation has its own runtime cache identity;
 * ordinary builds keep these ids readable without paying for increments. */
typedef enum dream_counter {
    DREAM_COUNT_WEAK_REGISTER = 0,
    DREAM_COUNT_WEAK_INVALIDATE,
    DREAM_COUNT_REGION_OBJECTS,
    DREAM_COUNT_LIMIT
} dream_counter;
extern uint64_t dream_runtime_counters[DREAM_COUNT_LIMIT];
DREAM_ALWAYS_INLINE void dream_count(dream_counter k, uint64_t n) {
#if defined(DREAM_RUNTIME_COUNTERS) && DREAM_RUNTIME_COUNTERS
    uint64_t v = __atomic_load_n(&dream_runtime_counters[k], __ATOMIC_RELAXED);
    __atomic_store_n(&dream_runtime_counters[k], v + n, __ATOMIC_RELAXED);
#else
    (void)k;
    (void)n;
#endif
}
int64_t debug_get_runtime_counter(int32_t id);

DREAM_ALWAYS_INLINE const dream_type_info *dream_object_info(dream_ptr p) {
    const dream_type_info *info;
    memcpy(&info, (char *)dream_p(p) - DREAM_BLOCK_HEADER + sizeof(dream_size), sizeof(info));
    return info;
}

DREAM_ALWAYS_INLINE void dream_set_type(dream_ptr ptr, const dream_type_info *info) {
    uintptr_t header = (uintptr_t)dream_p(ptr) - DREAM_BLOCK_HEADER;
    memcpy((void *)(header + sizeof(dream_size)), &info, sizeof(info));
}
#endif
