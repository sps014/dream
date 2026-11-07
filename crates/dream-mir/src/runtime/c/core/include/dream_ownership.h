#ifndef DREAM_OWNERSHIP_H
#define DREAM_OWNERSHIP_H

typedef struct dream_type_info {
    void (*visit)(dream_ptr);
    void (*finalize)(dream_ptr);
    void (*clear)(dream_ptr);
    void (*reclaim)(dream_ptr);
    int32_t tag;
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
    DREAM_COUNT_RETAIN,
    DREAM_COUNT_RELEASE,
    DREAM_COUNT_ENQUEUE,
    DREAM_COUNT_READY_ALLOC,
    DREAM_COUNT_TIMER_ALLOC,
    DREAM_COUNT_FOREIGN_ALLOC,
    DREAM_COUNT_WORKER_JOB_ALLOC,
    DREAM_COUNT_PUBLISH_VISIT,
    DREAM_COUNT_LIMIT
} dream_counter;
/* Heap gauges read through the same ids, measured by a heap walk on each read in every build.
 * Only native heaps are walked; wasm32 reads 0. */
enum {
    DREAM_GAUGE_HEAP_COMMITTED = DREAM_COUNT_LIMIT,
    DREAM_GAUGE_HEAP_LIVE_BYTES,
    DREAM_GAUGE_HEAP_FREE_BYTES,
    DREAM_GAUGE_LIMIT
};
int64_t dream_heap_gauge(int32_t id);
extern uint64_t dream_runtime_counters[DREAM_COUNT_LIMIT];
DREAM_ALWAYS_INLINE void dream_count(dream_counter k, uint64_t n) {
#if defined(DREAM_RUNTIME_COUNTERS) && DREAM_RUNTIME_COUNTERS
    __atomic_fetch_add(&dream_runtime_counters[k], n, __ATOMIC_RELAXED);
#else
    (void)k;
    (void)n;
#endif
}
int64_t debug_get_runtime_counter(int32_t id);

DREAM_ALWAYS_INLINE int32_t dream_tag_kind(int32_t tag) {
    tag &= TAG_VALUE_MASK;
    if (tag & TAG_ARRAY_KIND) { return TAG_ARRAY; }
    if (tag & TAG_FUTURE_KIND) { return TAG_FUTURE; }
    return tag;
}

DREAM_ALWAYS_INLINE const dream_type_info *dream_object_info(dream_ptr p) {
    return p ? dream_type_info_for_tag(__atomic_load_n(dream_tag_word(p), __ATOMIC_RELAXED) & TAG_VALUE_MASK) : NULL;
}

DREAM_ALWAYS_INLINE void dream_set_type(dream_ptr ptr, const dream_type_info *info) {
    if (!info) { return; }
    int32_t *word = dream_tag_word(ptr);
    int32_t tag = __atomic_load_n(word, __ATOMIC_RELAXED);
    if ((tag & TAG_VALUE_MASK) == info->tag) { return; }
    /* New identities are installed before publication; reallocating existing storage keeps
     * its identity. Shared/weak flags belong to the allocation, not its descriptor. */
    if ((tag & TAG_VALUE_MASK) == 0) { tag |= TAG_SHARED; }
    __atomic_store_n(word, (tag & ~TAG_VALUE_MASK) | info->tag, __ATOMIC_RELAXED);
}
#endif
