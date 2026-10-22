#include "dream_core.h"

#include <stdlib.h>
#include <time.h>

#ifndef DREAM_WASM32
#include "dream_thread.h"
#endif

#define KIND_HOST FUTURE_KIND_HOST
#define KIND_ALL FUTURE_KIND_ALL
#define KIND_RACE FUTURE_KIND_RACE

#include "node_pool.h"

#include "ready_queue.h"

static _Thread_local Node *timer_head;
static _Thread_local dream_ptr poll_current;
static _Thread_local int32_t poll_drop_start_retain;

#include "async_frame.h"

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
    dream_ptr context = ptr_at(f, F_CONTEXT)[0];
    ptr_at(f, F_CONTEXT)[0] = 0;
    /* The context is a CancellationToken: its last owner must run the typed glue that
     * releases the token's source, which a bare free skips. */
    dream_release_object(context);
    if (i32_at(f, F_KIND)[0] == 0 && ptr_at(f, F_CHILDREN)[0]) {
        dream_cancellation_release((uintptr_t)ptr_at(f, F_CHILDREN)[0]);
        ptr_at(f, F_CHILDREN)[0] = 0;
    }
    int32_t kind = i32_at(f, F_KIND)[0];
    if (kind == KIND_ALL || kind == KIND_RACE) {
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
    if (!f || i32_at(f, F_QUEUED)[0] || status_of(f) || i32_at(f, F_NEXT)[0]) {
        return;
    }
    /* A yield signal is owned by its channel/awaiter, with no poll or queue entry.
     * Giving it a scheduler token would keep abandoned cursors alive forever. */
    if (i32_at(f, F_KIND)[0] == KIND_HOST && i32_at(f, F_POLL)[0] == 0) {
        i32_at(f, F_NEXT)[0] = 2;
        return;
    }
    i32_at(f, F_NEXT)[0] = 1;
    dream_retain(f);
    kind = i32_at(f, F_KIND)[0];
    if (kind == KIND_ALL || kind == KIND_RACE) {
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

#ifdef DREAM_WASM32
DREAM_WASM_EXPORT(DREAM_SYM_NEW_FUTURE)
#endif
dream_ptr dream_new_future(dream_size size, int32_t poll, int32_t kind) {
    dream_size bytes = size < F_SLOTS ? F_SLOTS : size;
    /* Private: a frame lives on its scheduler's thread, and the one path that hands it to
     * another thread (dream_complete_foreign) publishes it first. */
    dream_ptr p = dream_malloc(bytes, TAG_FUTURE);
    memset(dream_p(p), 0, (size_t)bytes);
    i32_at(p, F_POLL)[0] = poll;
    i32_at(p, F_KIND)[0] = kind;
    if (poll_current) {
        dream_ptr context = ptr_at(poll_current, F_CONTEXT)[0];
        dream_retain(context);
        ptr_at(p, F_CONTEXT)[0] = context;
    }
    return p;
}

/* Pool callbacks must stay retained through the matching host completion. */
void dream_task_keep_alive(dream_ptr callback) { (void)callback; }

int32_t dream_task_has_context(void) {
    return poll_current && ptr_at(poll_current, F_CONTEXT)[0] != 0;
}

dream_ptr dream_task_context(void) {
    dream_ptr context = poll_current ? ptr_at(poll_current, F_CONTEXT)[0] : 0;
    dream_retain(context);
    return context;
}

void dream_task_attach(dream_ptr future, dream_ptr token) {
    dream_retain(token);
    dream_release_object(ptr_at(future, F_CONTEXT)[0]);
    ptr_at(future, F_CONTEXT)[0] = token;
}

int32_t dream_task_is_protected(dream_ptr future) {
    return future && i32_at(future, F_KIND)[0] == 0 && ptr_at(future, F_CHILDREN)[0] != 0;
}

void dream_task_protect(dream_ptr owner) {
    uintptr_t source = *(uintptr_t *)dream_p(owner);
    if (!poll_current || ptr_at(poll_current, F_CHILDREN)[0]) { return; }
    dream_cancellation_retain(source);
    ptr_at(poll_current, F_CHILDREN)[0] = (dream_ptr)source;
}

/* An event is registered synchronously, before a caller can abandon its wait. */
dream_ptr dream_event_new(void) {
    dream_ptr f = dream_new_future(F_SLOTS, 0, KIND_HOST);
    i32_at(f, F_NEXT)[0] = 1;
    dream_retain(f);
#ifndef DREAM_WASM32
    dream_foreign_work_begin(f);
#endif
    return f;
}

void dream_event_complete(dream_ptr f) {
#ifndef DREAM_WASM32
    /* Only the owner stores its own scheduler and claimers only replace it, so a lock-free read
     * decides locality. A completer that claims first owns the hand-back through foreign_drain. */
    int local = __atomic_load_n(scheduler_at(f), __ATOMIC_ACQUIRE) == &sched;
    if (!local) { dream_complete_foreign(f, 0); return; }
    if (!dream_foreign_work_end(f)) { return; }
    if (status_of(f) == 2) { scheduler_drop_start_retain(f); return; }
#endif
    dream_async_complete(f, 0);
}

void dream_enqueue(dream_ptr f) {
    ReadyNode *n;
    if (!f || i32_at(f, F_QUEUED)[0] || status_of(f)) {
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
DREAM_WASM_EXPORT(DREAM_SYM_RESOLVE)
#endif
void dream_resolve(dream_ptr f, dream_ptr res) {
    dream_async_complete(f, (dream_result)(uintptr_t)res);
}

#ifdef DREAM_WASM32
/* A JS Promise outlives a cancelled scheduler token. Its own pin also keeps cancelled frames
 * readable so the adapter can skip allocating a result that the guest will never consume. */
DREAM_WASM_EXPORT(DREAM_SYM_HOST_PIN)
void dream_async_host_pin(dream_ptr f) { dream_retain(f); }

DREAM_WASM_EXPORT(DREAM_SYM_HOST_STATUS)
#endif
int32_t dream_async_host_status(dream_ptr f) { return status_of(f); }
#ifdef DREAM_WASM32

DREAM_WASM_EXPORT(DREAM_SYM_HOST_UNPIN)
void dream_async_host_unpin(dream_ptr f) { dream_release(f); }
#endif

void dream_async_complete(dream_ptr f, dream_result res) {
    dream_ptr w;
    int32_t wk;
    if (!f) { return; }
    if (status_of(f)) { return; }
    *result_at(f) = res;
    status_store(f, 1);
    w = ptr_at(f, F_WAKER)[0];
    if (w) {
        ptr_at(f, F_WAKER)[0] = 0;
        wk = i32_at(w, F_KIND)[0];
        if (wk == KIND_ALL || wk == KIND_RACE) {
            dream_combinator_progress(w, f);
        } else {
            dream_enqueue(w);
        }
    }
    scheduler_drop_start_retain(f);
}

/* A cancelled future takes the child it awaits down with it when nothing else can observe that
 * child: the awaiting frame's slot is its only owner besides the scheduler's start retain.
 * Children shared with another holder (a stored handle, a scope's job list, a guard) keep
 * running, as do children mid-poll, which are cancelling their own awaiter. */
static int awaited_child_unshared(dream_ptr child) {
    if (child == poll_current || status_of(child)) {
        return 0;
    }
    int32_t owners = 1 + (i32_at(child, F_NEXT)[0] == 1);
    return dream_rc_count(child) == owners;
}

void dream_cancel(dream_ptr f) {
    Node **link;
    int in_flight = 0;
    /* Scoped futures remain schedulable until every owned child has settled. */
    if (f && !status_of(f) && i32_at(f, F_KIND)[0] == 0 && ptr_at(f, F_CHILDREN)[0]) {
        dream_cancellation_cancel((uintptr_t)ptr_at(f, F_CHILDREN)[0]);
        return;
    }
    if (!f) {
        return;
    }
#ifndef DREAM_WASM32
    /* The host still holds an in-flight frame; its completion hands it back to drop the retain.
     * Reading F_SCHEDULER after winning the status CAS sees either the owner's scheduler or a
     * completer's sentinel, and both mean foreign_drain will drop the retain. */
    int32_t idle = 0;
    if (!__atomic_compare_exchange_n(i32_at(f, F_STATUS), &idle, 2, 0, __ATOMIC_ACQ_REL,
                                     __ATOMIC_ACQUIRE)) {
        return;
    }
    in_flight = __atomic_load_n(scheduler_at(f), __ATOMIC_ACQUIRE) != NULL;
#else
    if (status_of(f)) {
        return;
    }
    status_store(f, 2);
#endif
#ifdef DREAM_WASM32
    async_host_cancel(f);
#endif
    dream_ptr child = ptr_at(f, F_AWAITING)[0];
    int cancel_child = 0;
    if (child && ptr_at(child, F_WAKER)[0] == f) {
        ptr_at(child, F_WAKER)[0] = 0;
        cancel_child = awaited_child_unshared(child);
    }
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
    if (cancel_child) {
        dream_cancel(child);
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
        /* A cancelled frame never wrote the result slot the resumed parent would read. */
        if (DREAM_UNLIKELY(status_of(child) == 2)) {
            DREAM_PANIC_LITERAL(u"panic: awaited a cancelled future");
        }
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
DREAM_WASM_EXPORT(DREAM_SYM_RUN_LOOP)
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
            if (status_of(f)) {
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
#ifdef DREAM_COMPONENT
            void dream_component_wait_until(int64_t deadline);
            dream_component_wait_until(timer_head->due);
            int64_t now = timeNowNanos();
#else
            int64_t now = timer_head->due;
            vclock = now;
#endif
#else
            int64_t now = timeNowNanos();
            int64_t due = timer_head ? timer_head->due : 0;
            if (due > now) {
                dream_mutex_lock(&sched.mu);
                /* Re-check under the lock: a foreign completion between the drain above and
                 * this wait must not be slept through. */
                if (!sched.fq_head && !dream_callback_pending()) {
                    scheduler_park(due - now);
                }
                dream_mutex_unlock(&sched.mu);
            } else if (!timer_head) {
                /* No timers: park until a foreign completion signals. */
                dream_mutex_lock(&sched.mu);
                if (!sched.fq_head && !dream_callback_pending()) {
                    scheduler_park(-1);
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
#if defined(DREAM_WASM32) && !defined(DREAM_COMPONENT)
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

#ifdef DREAM_COMPONENT
void delayMs(dream_ptr future, int32_t milliseconds) {
    timer_arm(future, milliseconds > 0 ? milliseconds : 0);
}
#endif

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

/* Signals carry no payload and cannot run independently. The cursor owns them while the
 * producer is suspended; completion wakes the producer without speculative advancement. */
dream_ptr dream_yield_signal(void) {
    return dream_new_future(F_SLOTS, 0, KIND_HOST);
}

void dream_yield_signal_complete(dream_ptr signal) {
    dream_async_complete(signal, 0);
}

void dream_yield_run(dream_ptr producer, dream_ptr ready) {
    dream_start(producer);
    while (!status_of(ready)) {
        ReadyNode *previous = NULL;
        ReadyNode **link = &rq_head;
        while (*link && (*link)->f != producer) {
            previous = *link;
            link = &(*link)->next;
        }
        if (!*link) {
            DREAM_PANIC_LITERAL(u"synchronous iterator suspended without yielding");
            return;
        }
        ReadyNode *node = *link;
        *link = node->next;
        if (rq_tail == node) { rq_tail = previous; }
        ready_put(node);
        i32_at(producer, F_QUEUED)[0] = 0;
        dream_ptr outer_poll = poll_current;
        int32_t outer_drop = poll_drop_start_retain;
        poll_current = producer;
        poll_drop_start_retain = 0;
        int32_t poll = i32_at(producer, F_POLL)[0];
        ((int32_t (*)(dream_ptr))dream_ft_get(poll))(producer);
        int32_t drop = poll_drop_start_retain;
        poll_current = outer_poll;
        poll_drop_start_retain = outer_drop;
        if (drop) { dream_release(producer); }
    }
}
