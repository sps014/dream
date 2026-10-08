#ifndef DREAM_OWNERSHIP_H
#define DREAM_OWNERSHIP_H

typedef struct dream_type_info {
    void (*visit)(dream_ptr);
    void (*finalize)(dream_ptr);
    void (*clear)(dream_ptr);
    void (*reclaim)(dream_ptr);
    int32_t cycle_capable;
    /* The generated clear glue can only release references or unregister observers;
     * child finalizers are queued, so it cannot call user code while the gate is held. */
    int32_t clear_no_user_code;
} dream_type_info;

const dream_type_info *dream_builtin_type_info(int32_t tag);
const dream_type_info *dream_type_info_for_tag(int32_t tag);
void dream_cycle_register(dream_ptr ptr);
void dream_cycle_register_new(dream_ptr ptr);
int dream_cycle_construction_begin(int private_graph);
int dream_cycle_graph_begin(int private_graph);
void dream_cycle_graph_end(int token);
void dream_cycle_initialize_edge(dream_ptr owner, dream_ptr child);
void dream_visit_owned(dream_ptr ptr, void (*edge)(dream_ptr, void *), void *context);
void dream_visit_edge(dream_ptr ptr);
int dream_cycle_store_begin_slow(dream_ptr owner, dream_ptr child, int value, int tracked);
void dream_cycle_store_end_slow(void);
/* Returns one only when this operation owns a new gate acquisition. */
int dream_cycle_acquire(void);
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

/* Opt-in ownership attribution. Instrumentation has its own runtime cache identity;
 * ordinary builds keep these ids readable without paying for increments. */
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
    DREAM_COUNT_UNKNOWN_STORES = 12,
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
    /* The pointer denotes the payload; GCC's bounds analysis otherwise assumes its
     * allocation begins there and rejects access to the preceding runtime header. */
    uintptr_t header = (uintptr_t)dream_p(ptr) - DREAM_BLOCK_HEADER;
    memcpy((void *)(header + sizeof(dream_size)), &info, sizeof(info));
    if (tracked) { dream_cycle_register(ptr); }
}

/* Allocation initializes the count and clears the slot before installing a descriptor;
 * no previous registration or immortal owner can exist at this boundary. */
DREAM_ALWAYS_INLINE void dream_set_type_new(dream_ptr ptr, const dream_type_info *info) {
    uintptr_t header = (uintptr_t)dream_p(ptr) - DREAM_BLOCK_HEADER;
    memcpy((void *)(header + sizeof(dream_size)), &info, sizeof(info));
    if (info && info->cycle_capable) { dream_cycle_register_new(ptr); }
}

/* The gated path stays out of line so the barrier inlined into every field store is only the
 * two flag tests. */
DREAM_ALWAYS_INLINE int dream_cycle_store_begin(dream_ptr owner, dream_ptr child, int value) {
    int tracked = dream_cycle_tracked(owner) || dream_cycle_tracked(child);
    if (!(value || tracked)) { return 0; }
    return dream_cycle_store_begin_slow(owner, child, value, tracked);
}

DREAM_ALWAYS_INLINE void dream_cycle_store_end(int locked) {
    if (locked == 1) { dream_cycle_store_end_slow(); }
}

DREAM_ALWAYS_INLINE void dream_cycle_check_store(dream_ptr owner, dream_ptr child) {
    if (dream_cycle_tracked(owner) || dream_cycle_tracked(child)) { dream_cycle_check_store_slow(owner, child); }
}
#endif
