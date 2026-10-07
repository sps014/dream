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
void dream_cycle_register(dream_ptr ptr);
void dream_visit_owned(dream_ptr ptr, void (*edge)(dream_ptr, void *), void *context);
void dream_visit_edge(dream_ptr ptr);
void dream_cycle_store_begin_slow(dream_ptr owner, dream_ptr child, int value, int tracked);
void dream_cycle_store_end_slow(void);
void dream_cycle_enter(void);
void dream_cycle_leave(void);
int dream_cycle_release(dream_ptr ptr);
void dream_cycle_retain(dream_ptr ptr);
void dream_cycle_forget_slow(dream_ptr ptr);
void dream_cycle_check_store_slow(dream_ptr owner, dream_ptr child);
void dream_cycle_drain(void);
void dream_cycle_finish(void);
int dream_cycle_defer_destroy(dream_ptr ptr, void (*destroy)(dream_ptr));
void dream_cycle_postpone(int value);

/* Ownership-runtime event counts read by `Debug.runtime_counter(id)`. Every increment happens
 * on a path that already holds the cycle gate or the weak lock, so the cost stays below the
 * lock it accompanies; ids are part of the benchmark output format. */
typedef enum dream_counter {
    DREAM_COUNT_CYCLE_NODES = 0,
    DREAM_COUNT_CYCLE_FORGETS = 1,
    DREAM_COUNT_CANDIDATES = 2,
    DREAM_COUNT_LOOKUPS = 3,
    DREAM_COUNT_GATES = 4,
    DREAM_COUNT_EDGES = 5,
    DREAM_COUNT_TRIALS = 6,
    DREAM_COUNT_FORWARD_QUEUES = 7,
    DREAM_COUNT_DEFERRED = 8,
    DREAM_COUNT_WEAK_REGISTER = 9,
    DREAM_COUNT_WEAK_INVALIDATE = 10,
    DREAM_COUNT_REGION_OBJECTS = 11,
    DREAM_COUNT_LIMIT
} dream_counter;
extern uint64_t dream_runtime_counters[DREAM_COUNT_LIMIT];
DREAM_ALWAYS_INLINE void dream_count(dream_counter k, uint64_t n) {
    uint64_t v = __atomic_load_n(&dream_runtime_counters[k], __ATOMIC_RELAXED);
    __atomic_store_n(&dream_runtime_counters[k], v + n, __ATOMIC_RELAXED);
}
int64_t debug_get_runtime_counter(int32_t id);

DREAM_ALWAYS_INLINE const dream_type_info *dream_object_info(dream_ptr p) {
    const dream_type_info *info;
    memcpy(&info, (char *)dream_p(p) - DREAM_BLOCK_HEADER + sizeof(dream_size), sizeof(info));
    return info;
}

/* Collector work is keyed on the descriptor, so every store and allocation filters inline and
 * only a cycle-capable object reaches the out-of-line paths. A native header holds the 1-based
 * index of the object's collector node beside the ARC words (0 = not registered): barriers test
 * the header line they already touch, and the collector reaches the node without a lookup.
 * Only the collector writes the slot, under its gate; activation zeroes it. */
#ifndef DREAM_WASM32
#define DREAM_CYCLE_SLOT_FROM_DATA 16
DREAM_ALWAYS_INLINE uint32_t *dream_cycle_slot(dream_ptr p) {
    return (uint32_t *)((char *)dream_p(p) - DREAM_CYCLE_SLOT_FROM_DATA);
}
#endif

DREAM_ALWAYS_INLINE int dream_cycle_tracked(dream_ptr p) {
    if (!p) { return 0; }
#ifdef DREAM_WASM32
    const dream_type_info *info = dream_object_info(p);
    return info && info->cycle_capable;
#else
    return *dream_cycle_slot(p) != 0;
#endif
}

DREAM_ALWAYS_INLINE void dream_cycle_forget(dream_ptr ptr) {
    if (dream_cycle_tracked(ptr)) { dream_cycle_forget_slow(ptr); }
}

DREAM_ALWAYS_INLINE void dream_set_type(dream_ptr ptr, const dream_type_info *info) {
    int tracked = info && info->cycle_capable;
#ifndef DREAM_WASM32
    if (!tracked) { dream_cycle_forget(ptr); }
#endif
    memcpy((char *)dream_p(ptr) - DREAM_BLOCK_HEADER + sizeof(dream_size), &info, sizeof(info));
    if (tracked) { dream_cycle_register(ptr); }
}

/* The gated path stays out of line so the barrier inlined into every field store is only the
 * two flag tests. */
DREAM_ALWAYS_INLINE int dream_cycle_store_begin(dream_ptr owner, dream_ptr child, int value) {
    int tracked = dream_cycle_tracked(owner) || dream_cycle_tracked(child);
    if (!(value || tracked)) { return 0; }
    dream_cycle_store_begin_slow(owner, child, value, tracked);
    return 1;
}

DREAM_ALWAYS_INLINE void dream_cycle_store_end(int locked) {
    if (locked) { dream_cycle_store_end_slow(); }
}

DREAM_ALWAYS_INLINE void dream_cycle_check_store(dream_ptr owner, dream_ptr child) {
    if (dream_cycle_tracked(owner) || dream_cycle_tracked(child)) { dream_cycle_check_store_slow(owner, child); }
}
#endif
