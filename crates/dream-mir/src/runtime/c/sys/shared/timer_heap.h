#ifndef DREAM_TIMER_HEAP_H
#define DREAM_TIMER_HEAP_H
#include "dream_platform_internal.h"

/* Per-thread binary min-heap of armed timers keyed by (due, seq). `seq` grows monotonically,
 * so equal deadlines fire in arming order. Each armed frame stores its heap index + 1 in
 * F_REMAINING, which only combinators otherwise use and combinators are never armed. */
typedef struct TimerEntry {
    int64_t due;
    uint64_t seq;
    dream_ptr f;
} TimerEntry;

#define TIMER_INLINE_CAPACITY 16

static _Thread_local TimerEntry timer_inline[TIMER_INLINE_CAPACITY];
static _Thread_local TimerEntry *timer_spill;
static _Thread_local uint32_t timer_count;
static _Thread_local uint32_t timer_capacity;
static _Thread_local uint64_t timer_seq;

static inline TimerEntry *timer_entries(void) { return timer_spill ? timer_spill : timer_inline; }

static inline int timer_before(const TimerEntry *a, const TimerEntry *b) {
    return a->due < b->due || (a->due == b->due && a->seq < b->seq);
}

static inline void timer_place(TimerEntry *heap, uint32_t i, TimerEntry entry) {
    heap[i] = entry;
    i32_at(entry.f, F_REMAINING)[0] = (int32_t)(i + 1);
}

static void timer_sift_up(TimerEntry *heap, uint32_t i) {
    TimerEntry entry = heap[i];
    while (i > 0) {
        uint32_t parent = (i - 1) / 2;
        if (!timer_before(&entry, &heap[parent])) { break; }
        timer_place(heap, i, heap[parent]);
        i = parent;
    }
    timer_place(heap, i, entry);
}

static void timer_sift_down(TimerEntry *heap, uint32_t i) {
    TimerEntry entry = heap[i];
    for (;;) {
        uint32_t child = 2 * i + 1;
        if (child >= timer_count) { break; }
        if (child + 1 < timer_count && timer_before(&heap[child + 1], &heap[child])) { child++; }
        if (!timer_before(&heap[child], &entry)) { break; }
        timer_place(heap, i, heap[child]);
        i = child;
    }
    timer_place(heap, i, entry);
}

static void timer_grow(void) {
    uint32_t capacity = timer_spill ? timer_capacity * 2 : TIMER_INLINE_CAPACITY * 4;
    TimerEntry *grown = dream_platform_current->allocate((size_t)capacity * sizeof(TimerEntry));
    if (!grown) { DREAM_PANIC_LITERAL(u"panic: scheduler allocation failed"); }
    dream_count(DREAM_COUNT_TIMER_ALLOC, 1);
    memcpy(grown, timer_entries(), (size_t)timer_count * sizeof(TimerEntry));
    if (timer_spill) { dream_platform_current->deallocate(timer_spill); }
    timer_spill = grown;
    timer_capacity = capacity;
}

static void timer_push(dream_ptr f, int64_t due) {
    uint32_t limit = timer_spill ? timer_capacity : TIMER_INLINE_CAPACITY;
    if (timer_count == limit) { timer_grow(); }
    TimerEntry *heap = timer_entries();
    heap[timer_count] = (TimerEntry){due, timer_seq++, f};
    timer_sift_up(heap, timer_count++);
}

static inline const TimerEntry *timer_peek(void) { return timer_count ? &timer_entries()[0] : NULL; }

static void timer_remove_at(uint32_t i) {
    TimerEntry *heap = timer_entries();
    i32_at(heap[i].f, F_REMAINING)[0] = 0;
    if (i == --timer_count) { return; }
    heap[i] = heap[timer_count];
    timer_sift_down(heap, i);
    timer_sift_up(heap, i);
}

static dream_ptr timer_pop(void) {
    dream_ptr f = timer_entries()[0].f;
    timer_remove_at(0);
    return f;
}

static void timer_cancel(dream_ptr f) {
    int32_t kind = i32_at(f, F_KIND)[0];
    if (kind == FUTURE_KIND_ALL || kind == FUTURE_KIND_RACE) { return; }
    int32_t slot = i32_at(f, F_REMAINING)[0];
    if (slot > 0 && (uint32_t)slot <= timer_count && timer_entries()[slot - 1].f == f) {
        timer_remove_at((uint32_t)slot - 1);
    }
}

/* Called once the run loop drains: no timer can be armed, so the spill buffer goes back. */
static void timer_finish(void) {
    if (timer_spill) { dream_platform_current->deallocate(timer_spill); }
    timer_spill = NULL;
    timer_capacity = 0;
}
#endif
