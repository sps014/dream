#include "dream_core.h"

#include <stdlib.h>
#include <time.h>

#ifndef DREAM_WASM32
#include "dream_thread.h"
#endif

#define KIND_HOST FUTURE_KIND_HOST
#define KIND_ALL FUTURE_KIND_ALL
#define KIND_ANY FUTURE_KIND_ANY

#include "node_pool.h"

#include "ready_queue.h"

static _Thread_local Node *timer_head;
static _Thread_local dream_ptr poll_current;
static _Thread_local int32_t poll_drop_start_retain;

static int32_t *i32_at(dream_ptr p, int32_t off) {
    return (int32_t *)((char *)dream_p(p) + off);
}

static dream_ptr *ptr_at(dream_ptr p, int32_t off) {
    return (dream_ptr *)((char *)dream_p(p) + off);
}

static dream_result *result_at(dream_ptr p) {
    return (dream_result *)((char *)dream_p(p) + F_RESULT);
}

/* The 4-byte length prefix leaves native 8-byte elements only 4-aligned. */
static dream_ptr arr_get(dream_ptr arr, int32_t i) {
    dream_ptr element;
    memcpy(&element, (char *)dream_p(arr) + LEN_PREFIX_SIZE + (size_t)i * sizeof(dream_ptr), sizeof(element));
    return element;
}

void *dream_ft_get(int32_t i);

__attribute__((weak)) void *dream_fd_get(int32_t i) {
    (void)i;
    return 0;
}

void dream_future_fini(dream_ptr f) {
    int32_t poll;
    void (*drop)(dream_ptr);
    if (!f) {
        return;
    }
    int32_t kind = i32_at(f, F_KIND)[0];
    if (kind == KIND_ALL || kind == KIND_ANY) {
        dream_ptr children = ptr_at(f, F_CHILDREN)[0];
        int32_t count = i32_at(f, F_COUNT)[0];
        for (int32_t i = 0; children && i < count; ++i) {
            dream_ptr child = arr_get(children, i);
            if (child && ptr_at(child, F_WAKER)[0] == f) {
                ptr_at(child, F_WAKER)[0] = 0;
            }
        }
    }
    const dream_type_info *info = dream_object_info(f);
    if (info && info->clear) {
        info->clear(f);
        return;
    }
    poll = i32_at(f, F_POLL)[0];
    if (poll <= 0) { return; }
    drop = (void (*)(dream_ptr))dream_fd_get(poll);
    if (drop) {
        drop(f);
    }
}
static void combinator_progress(dream_ptr w, dream_ptr child);

/* F_NEXT: 0 = lazy, 1 = started and the scheduler holds a retain, 2 = started retain dropped.
 * The scheduler retain is what makes Promise.start(borrow) and last-use of an awaited child
 * safe: the runqueue / timer / foreign tables store raw pointers. Drop is deferred while the
 * future's own poll is on the stack so complete cannot free __self mid-function. */
static void scheduler_drop_start_retain(dream_ptr f) {
    if (!f || i32_at(f, F_NEXT)[0] != 1) {
        return;
    }
    i32_at(f, F_NEXT)[0] = 2;
    if (f == poll_current) {
        poll_drop_start_retain = 1;
        return;
    }
    dream_release(f);
}

/* Futures are lazy: constructing one does not run it. It runs when first awaited, started by a
 * combinator it was passed to, or explicitly scheduled via dream_start (Promise.start). The
 * F_NEXT word doubles as the started latch: without it, awaiting a future that is already
 * running-but-suspended would re-enqueue it mid-execution and resume into garbage. */
void dream_start(dream_ptr f) {
    int32_t kind;
    if (!f || i32_at(f, F_QUEUED)[0] || i32_at(f, F_STATUS)[0] || i32_at(f, F_NEXT)[0]) {
        return;
    }
    i32_at(f, F_NEXT)[0] = 1;
    dream_retain(f);
    kind = i32_at(f, F_KIND)[0];
    if (kind == KIND_ALL || kind == KIND_ANY) {
        /* Combinators are passive (no poll fn); starting one starts its members. Members that
         * are themselves combinators recurse through dream_start above. */
        dream_ptr kids = ptr_at(f, F_CHILDREN)[0];
        int32_t n = i32_at(f, F_COUNT)[0];
        int32_t i;
        for (i = 0; i < n; i++) {
            dream_start(arr_get(kids, i));
        }
        return;
    }
    dream_enqueue(f);
}

#ifdef DREAM_WASM32
static int64_t vclock;
#endif

#ifndef DREAM_WASM32
#include "../native/foreign_queue.h"

#endif

/* Off-thread completion flips F_STATUS after storing the result. */
static int32_t status_of(dream_ptr f) {
#ifdef DREAM_WASM32
    return i32_at(f, F_STATUS)[0];
#else
    return __atomic_load_n(i32_at(f, F_STATUS), __ATOMIC_ACQUIRE);
#endif
}

#ifdef DREAM_WASM32
__attribute__((export_name(DREAM_SYM_NEW_FUTURE)))
#endif
dream_ptr dream_new_future(dream_size size, int32_t poll, int32_t kind) {
    dream_size bytes = size < F_SLOTS ? F_SLOTS : size;
    /* Private: a frame lives on its scheduler's thread, and the one path that hands it to
     * another thread (dream_complete_foreign) publishes it first. */
    dream_ptr p = dream_malloc(bytes, TAG_FUTURE);
    memset(dream_p(p), 0, (size_t)bytes);
    i32_at(p, F_POLL)[0] = poll;
    i32_at(p, F_KIND)[0] = kind;
    return p;
}

void dream_enqueue(dream_ptr f) {
    ReadyNode *n;
    if (!f || i32_at(f, F_QUEUED)[0]) {
        return;
    }
    dream_count(DREAM_COUNT_ENQUEUE, 1);
    i32_at(f, F_QUEUED)[0] = 1;
    n = ready_take(f);
    if (!rq_tail) {
        rq_head = n;
        rq_tail = n;
        return;
    }
    rq_tail->next = n;
    rq_tail = n;
}

#ifdef DREAM_WASM32
__attribute__((export_name(DREAM_SYM_RESOLVE)))
#endif
void dream_resolve(dream_ptr f, dream_ptr res) {
    dream_async_complete(f, (dream_result)(uintptr_t)res);
}

void dream_async_complete(dream_ptr f, dream_result res) {
    dream_ptr w;
    int32_t wk;
    if (!f) { return; }
    if (i32_at(f, F_STATUS)[0]) { return; }
    *result_at(f) = res;
    i32_at(f, F_STATUS)[0] = 1;
    w = ptr_at(f, F_WAKER)[0];
    if (w) {
        ptr_at(f, F_WAKER)[0] = 0;
        wk = i32_at(w, F_KIND)[0];
        if (wk == KIND_ALL || wk == KIND_ANY) {
            combinator_progress(w, f);
        } else {
            dream_enqueue(w);
        }
    }
    scheduler_drop_start_retain(f);
}

void dream_cancel(dream_ptr f) {
    Node **link;
    int in_flight = 0;
    if (!f) {
        return;
    }
#ifndef DREAM_WASM32
    /* The host still holds an in-flight frame; its completion hands it back to drop the retain. */
    dream_mutex_lock(&foreign_claim_mu);
    if (i32_at(f, F_STATUS)[0]) {
        dream_mutex_unlock(&foreign_claim_mu);
        return;
    }
    i32_at(f, F_STATUS)[0] = 2;
    in_flight = *scheduler_at(f) != NULL;
    dream_mutex_unlock(&foreign_claim_mu);
#else
    if (i32_at(f, F_STATUS)[0]) {
        return;
    }
    i32_at(f, F_STATUS)[0] = 2;
#endif
    ptr_at(f, F_WAKER)[0] = 0;
    /* Queues borrow the scheduler token. Unlink before dropping it so cancelled
     * frames cannot leave a raw queue pointer into reclaimed storage. */
    ReadyNode *previous = NULL;
    for (ReadyNode **ready_link = &rq_head; *ready_link;) {
        ReadyNode *node = *ready_link;
        if (node->f == f) {
            *ready_link = node->next;
            if (rq_tail == node) { rq_tail = previous; }
            ready_put(node);
        } else {
            previous = node;
            ready_link = &node->next;
        }
    }
    i32_at(f, F_QUEUED)[0] = 0;
    for (link = &timer_head; *link;) {
        Node *node = *link;
        if (node->f == f) {
            *link = node->next;
            timer_node_put(node);
        } else { link = &node->next; }
    }
    if (!in_flight) {
        scheduler_drop_start_retain(f);
    }
}

int32_t dream_async_await(dream_ptr future, dream_result *dest, int32_t resume_pc) {
    (void)resume_pc;
    if (!future) {
        if (dest) {
            *dest = 0;
        }
        return 1;
    }
    if (status_of(future)) {
        if (dest) {
            *dest = *result_at(future);
        }
        return 1;
    }
    return 0;
}

void dream_async_set_waker(dream_ptr future, dream_ptr self) {
    if (future) {
        ptr_at(future, F_WAKER)[0] = self;
    }
}

void dream_await(dream_ptr parent, dream_ptr child) {
    if (!parent) {
        return;
    }
    ptr_at(parent, F_AWAITING)[0] = child;
    if (!child) {
        dream_enqueue(parent);
        return;
    }
    /* A foreign completion landing after this check is routed by foreign_drain on this thread,
     * which reads the waker only once it is set. */
    if (status_of(child)) {
        dream_enqueue(parent);
        return;
    }
    ptr_at(child, F_WAKER)[0] = parent;
    /* Awaiting a lazy future is what launches it. */
    dream_start(child);
}

int dream_async_pending(void) {
#ifndef DREAM_WASM32
    if (foreign_outstanding()) {
        return 1;
    }
#endif
    return rq_head != NULL || timer_head != NULL;
}

#ifdef DREAM_WASM32
__attribute__((export_name(DREAM_SYM_RUN_LOOP)))
#endif
void dream_run_loop(void) {
#ifndef DREAM_WASM32
    dream_callback_set_waker(callback_wake, &sched);
#endif
    for (;;) {
#ifndef DREAM_WASM32
        foreign_drain();
        dream_callback_drain();
#endif
        while (rq_head) {
            ReadyNode *n = rq_head;
            dream_ptr f = n->f;
            rq_head = n->next;
            if (!rq_head) {
                rq_tail = NULL;
            }
            ready_put(n);
            i32_at(f, F_QUEUED)[0] = 0;
            if (i32_at(f, F_STATUS)[0]) {
                continue;
            }
            {
                int32_t poll = i32_at(f, F_POLL)[0];
                if (poll > 0) {
                    poll_current = f;
                    poll_drop_start_retain = 0;
                    ((int32_t (*)(dream_ptr))dream_ft_get(poll))(f);
#ifndef DREAM_WASM32
                    dream_callback_drain();
#endif
                    poll_current = 0;
                    if (poll_drop_start_retain) {
                        poll_drop_start_retain = 0;
                        dream_release(f);
                    }
                }
            }
        }
#ifndef DREAM_WASM32
        foreign_drain();
        /* Drain can enqueue waiters onto rq; run them before parking or returning. */
        if (rq_head) {
            continue;
        }
        if (!timer_head && !foreign_outstanding()) {
            ready_finish();
            return;
        }
#else
        if (!timer_head) {
            ready_finish();
            return;
        }
#endif
        {
#ifdef DREAM_WASM32
            int64_t now = timer_head->due;
            vclock = now;
#else
            int64_t now = timeNowNanos();
            int64_t due = timer_head ? timer_head->due : 0;
            if (due > now) {
                dream_mutex_lock(&sched.mu);
                /* Re-check under the lock: a foreign completion between the drain above and
                 * this wait must not be slept through. */
                if (!sched.fq_head && !dream_callback_pending()) {
                    dream_cond_wait_ns(&sched.cv, &sched.mu, due - now);
                }
                dream_mutex_unlock(&sched.mu);
            } else if (!timer_head) {
                /* No timers: park until a foreign completion signals. */
                dream_mutex_lock(&sched.mu);
                if (!sched.fq_head && !dream_callback_pending()) {
                    dream_cond_wait(&sched.cv, &sched.mu);
                }
                dream_mutex_unlock(&sched.mu);
            }
            now = timeNowNanos();
#endif
            while (timer_head && timer_head->due <= now) {
                Node *n = timer_head;
                timer_head = n->next;
                dream_async_complete(n->f, 0);
                timer_node_put(n);
            }
        }
    }
}

/* The timer holds `f` raw: the awaiter owns it, and `dream_cancel` unlinks the node. */
static void timer_arm(dream_ptr f, int32_t ms) {
    Node *n = timer_node_take();
    Node **pp;
    n->f = f;
#ifdef DREAM_WASM32
    n->due = vclock + (int64_t)ms * 1000000LL;
#else
    n->due = timeNowNanos() + (int64_t)ms * 1000000LL;
#endif
    pp = &timer_head;
    while (*pp && (*pp)->due <= n->due) {
        pp = &(*pp)->next;
    }
    n->next = *pp;
    *pp = n;
}

dream_ptr dream_sleep(int32_t ms) {
    dream_ptr f = dream_new_future((int32_t)F_SLOTS, HOST_POLL_INDEX, KIND_HOST);
    timer_arm(f, ms);
    return f;
}

#ifndef DREAM_WASM32
/* Native `@async_host` side of `@runtime("delayMs")` (wasm32 imports JS `setTimeout`). The
 * bridge future itself goes on the timer queue, so the await suspends until it is due; a
 * blocking or future-returning host would complete the bridge inline and skip the wait. Returns
 * 0 because nothing is deferred to another thread. */
int32_t delayMsAsync(dream_ptr future, int32_t ms) {
    timer_arm(future, ms);
    return 0;
}
#endif

typedef struct {
    void (*copy)(dream_ptr, dream_ptr);
    dream_result (*clone)(dream_result);
    const dream_type_info *array_info;
} CombinatorState;
static CombinatorState *combinator_state(dream_ptr f) {
    return (CombinatorState *)((char *)dream_p(f) + F_SLOTS);
}
static void combinator_progress(dream_ptr w, dream_ptr child) {
    int32_t kind = i32_at(w, F_KIND)[0];
    if (i32_at(w, F_STATUS)[0]) { return; }
    if (kind != KIND_ANY && --i32_at(w, F_REMAINING)[0] > 0) { return; }
    int32_t n = i32_at(w, F_COUNT)[0];
    dream_ptr kids = ptr_at(w, F_CHILDREN)[0];
    for (int32_t i = 0; i < n; ++i) {
        dream_ptr c = arr_get(kids, i);
        if (c && ptr_at(c, F_WAKER)[0] == w) { ptr_at(c, F_WAKER)[0] = 0; }
    }
    CombinatorState *state = combinator_state(w);
    if (kind == KIND_ANY) {
        memcpy((char *)dream_p(w) + F_WIDE, (char *)dream_p(child) + F_WIDE, 8);
        dream_async_complete(w, state->clone(*result_at(child)));
    } else {
        int32_t es = i32_at(w, F_ESIZE)[0];
        dream_ptr out = dream_array_new(n, es);
        dream_set_type(out, state->array_info);
        for (int32_t i = 0; i < n; ++i) {
            dream_ptr c = arr_get(kids, i);
            if (c) { state->copy((dream_ptr)((char *)dream_p(out) + LEN_PREFIX_SIZE + (size_t)i * (size_t)es), c); }
        }
        dream_async_complete(w, (dream_result)(uintptr_t)out);
    }
}

static dream_ptr combinator_new(dream_ptr arr, int32_t kind, int32_t esize, void (*copy)(dream_ptr, dream_ptr), dream_result (*clone)(dream_result), const dream_type_info *array_info) {
    int32_t n = arr ? dream_i32(arr)[0] : 0;
    int32_t i;
    dream_ptr w = dream_new_future(F_SLOTS + sizeof(CombinatorState), HOST_POLL_INDEX, kind);
    *combinator_state(w) = (CombinatorState){copy, clone, array_info};
    dream_retain(arr);
    ptr_at(w, F_CHILDREN)[0] = arr;
    i32_at(w, F_COUNT)[0] = n;
    i32_at(w, F_REMAINING)[0] = n;
    i32_at(w, F_ESIZE)[0] = esize > 0 ? esize : 4;
    if (n == 0 && kind == KIND_ALL) {
        dream_ptr out = dream_array_new(0, esize);
        dream_set_type(out, array_info);
        dream_async_complete(w, (dream_result)(uintptr_t)out);
        return w;
    }
    for (i = 0; i < n; i++) {
        dream_ptr c = arr_get(arr, i);
        if (!c) {
            continue;
        }
        ptr_at(c, F_WAKER)[0] = w;
        if (i32_at(c, F_STATUS)[0]) {
            combinator_progress(w, c);
        }
    }
    return w;
}

dream_ptr dream_all(dream_ptr arr, int32_t esize, void (*copy)(dream_ptr, dream_ptr), const dream_type_info *info) { return combinator_new(arr, KIND_ALL, esize, copy, NULL, info); }

dream_ptr dream_any(dream_ptr arr, dream_result (*clone)(dream_result)) { return combinator_new(arr, KIND_ANY, 8, NULL, clone, NULL); }
