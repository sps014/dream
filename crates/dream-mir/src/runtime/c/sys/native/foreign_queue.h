#ifndef DREAM_FOREIGN_QUEUE_H
#define DREAM_FOREIGN_QUEUE_H
/* Foreign-thread completion for @async_host hosts. Each thread's scheduler owns the host work it
 * started: the frame records that scheduler in F_SCHEDULER, and the host thread only publishes
 * the result and hands the frame back. Waking, combinator progress and the scheduler retain stay
 * on the owning thread. A scheduler with work in flight never leaves dream_run_loop, so a frame
 * never names a scheduler whose thread has exited. Queue state and pending credits share its
 * mutex. Pending credits remain held until the completion is queued, keeping the TLS scheduler
 * alive.
 *
 * Claims take no global lock. A completer swaps F_SCHEDULER to SCHEDULER_CLAIMED, so exactly one
 * completer or the owner's dream_foreign_work_end wins. F_STATUS moves out of 0 only by CAS:
 * the winner publishes 1 after writing the result, while dream_cancel publishes 2. A cancel that
 * still sees a scheduler or the sentinel leaves the start retain to foreign_drain, which clears
 * the sentinel on the owner thread. The frame stays alive across the claim because its start
 * retain is dropped only by the owner after foreign_drain handles the queued node, and the node
 * is queued only after publication. The scheduler stays alive through the claimed pending
 * credit, which is returned under its mutex after queueing. */
typedef struct Scheduler {
    dream_mutex mu;
    dream_cond cv;
    Node *fq_head;
    Node *fq_tail;
    int32_t foreign_pending;
    /* The owner is blocked on cv. Wakers signal only then and clear it, so completions landing
     * while the owner runs cost no syscall. Guarded by mu. */
    int parked;
    /* An installed park hook replaces the condvar wait, so a window system can pump its events
     * while the scheduler idles. Guarded by mu; `wake` runs under mu so uninstalling cannot race a
     * wake into a torn-down event loop. */
    void (*park)(int64_t timeout_ns, void *context);
    void (*wake)(void *context);
    void *park_context;
} Scheduler;

#define SCHEDULER_CLAIMED ((Scheduler *)(uintptr_t)1)

static _Thread_local Scheduler sched = {.mu = DREAM_MUTEX_INIT, .cv = DREAM_COND_INIT};

static Scheduler **scheduler_at(dream_ptr f) {
    return (Scheduler **)ptr_at(f, F_SCHEDULER);
}

/* Caller holds s->mu. */
static void scheduler_signal(Scheduler *s) {
    if (s->parked) {
        s->parked = 0;
        dream_cond_signal(&s->cv);
    }
    if (s->wake) { s->wake(s->park_context); }
}

static void callback_wake(void *context) {
    Scheduler *s = context;
    dream_mutex_lock(&s->mu);
    scheduler_signal(s);
    dream_mutex_unlock(&s->mu);
}

void dream_sched_set_park(void (*park)(int64_t, void *), void (*wake)(void *), void *context) {
    dream_mutex_lock(&sched.mu);
    if (park && wake) {
        sched.park = park;
        sched.wake = wake;
        sched.park_context = context;
    } else {
        sched.park = NULL;
        sched.wake = NULL;
        sched.park_context = NULL;
    }
    dream_mutex_unlock(&sched.mu);
}

/* Caller holds sched.mu and has re-checked that nothing is queued. The hook runs unlocked; a wake
 * that lands before it blocks leaves an event queued, so the hook returns at once. */
static void scheduler_park(int64_t timeout_ns) {
    if (sched.park) {
        void (*park)(int64_t, void *) = sched.park;
        void *context = sched.park_context;
        dream_mutex_unlock(&sched.mu);
        park(timeout_ns, context);
        dream_mutex_lock(&sched.mu);
    } else {
        sched.parked = 1;
        if (timeout_ns < 0) {
            dream_cond_wait(&sched.cv, &sched.mu);
        } else {
            dream_cond_wait_ns(&sched.cv, &sched.mu, timeout_ns);
        }
        sched.parked = 0;
    }
}

void dream_foreign_work_begin(dream_ptr f) {
    /* Foreign code may start before returning its deferred flag. Publication must happen on
     * the owner while its private ARC obligations are still exclusive. */
    __atomic_store_n(&dream_rt_mt, 1, __ATOMIC_RELEASE);
    dream_publish(f);
    /* No other thread can name f until the host handoff, which orders this store before any
     * claim; taking the claim lock here only made the owner queue behind completing workers. */
    dream_mutex_lock(&sched.mu);
    sched.foreign_pending += 1;
    __atomic_store_n(scheduler_at(f), &sched, __ATOMIC_RELAXED);
    dream_mutex_unlock(&sched.mu);
}

/* Returns 1 when the owner reclaimed the frame; 0 when a completer already claimed it. */
int dream_foreign_work_end(dream_ptr f) {
    Scheduler *s = __atomic_load_n(scheduler_at(f), __ATOMIC_ACQUIRE);
    if (s == NULL || s == SCHEDULER_CLAIMED ||
        !__atomic_compare_exchange_n(scheduler_at(f), &s, NULL, 0, __ATOMIC_ACQ_REL,
                                     __ATOMIC_ACQUIRE)) {
        return 0;
    }
    dream_mutex_lock(&s->mu);
    s->foreign_pending -= 1;
    dream_mutex_unlock(&s->mu);
    return 1;
}

/* A cancelled frame is still handed back so its owner drops the scheduler retain the host was
 * relying on. */
static void complete_foreign(dream_ptr f, dream_result res, int owned) {
    Node *n;
    Scheduler *s;
    if (!f) {
        if (owned) { dream_release((dream_ptr)res); }
        return;
    }
    s = __atomic_load_n(scheduler_at(f), __ATOMIC_ACQUIRE);
    do {
        if (s == NULL || s == SCHEDULER_CLAIMED) {
            if (owned) { dream_release((dream_ptr)res); }
            return;
        }
    } while (!__atomic_compare_exchange_n(scheduler_at(f), &s, SCHEDULER_CLAIMED, 1,
                                          __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE));
    n = foreign_node_take();
    n->f = f;
    int discarded = __atomic_load_n(i32_at(f, F_STATUS), __ATOMIC_ACQUIRE) != 0;
    if (!discarded) {
        *result_at(f) = res;
        dream_publish(f);
        int32_t idle = 0;
        discarded = !__atomic_compare_exchange_n(i32_at(f, F_STATUS), &idle, 1, 0,
                                                 __ATOMIC_RELEASE, __ATOMIC_ACQUIRE);
        if (discarded) {
            *result_at(f) = 0;
        }
    }
    dream_mutex_lock(&s->mu);
    s->foreign_pending -= 1;
    int wake = s->fq_head == NULL;
    if (s->fq_tail) {
        s->fq_tail->next = n;
    } else {
        s->fq_head = n;
    }
    s->fq_tail = n;
    /* A nonempty queue already has a wake in flight or an owner draining it. */
    if (wake) {
        scheduler_signal(s);
    }
    dream_mutex_unlock(&s->mu);
    if (owned && discarded) { dream_release((dream_ptr)res); }
}

void dream_complete_foreign(dream_ptr f, dream_result res) { complete_foreign(f, res, 0); }
void dream_complete_foreign_owned(dream_ptr f, dream_ptr res) { complete_foreign(f, (dream_result)res, 1); }

static void foreign_drain(void) {
    for (;;) {
        dream_mutex_lock(&sched.mu);
        Node *n = sched.fq_head;
        sched.fq_head = sched.fq_tail = NULL;
        dream_mutex_unlock(&sched.mu);
        if (!n) {
            return;
        }
        do {
            Node *next = n->next;
            dream_ptr f = n->f;
            foreign_node_put(n);
            __atomic_store_n(scheduler_at(f), NULL, __ATOMIC_RELAXED);
            dream_ptr w = ptr_at(f, F_WAKER)[0];
            if (w) {
                ptr_at(f, F_WAKER)[0] = 0;
                int32_t wk = i32_at(w, F_KIND)[0];
                if (wk == FUTURE_KIND_ALL || wk == FUTURE_KIND_RACE) {
                    dream_combinator_progress(w, f);
                } else {
                    dream_enqueue(w);
                }
            }
            scheduler_drop_start_retain(f);
            n = next;
        } while (n);
    }
}

static int foreign_outstanding(void) {
    dream_mutex_lock(&sched.mu);
    int outstanding = sched.fq_head != NULL || sched.foreign_pending != 0;
    dream_mutex_unlock(&sched.mu);
    return outstanding;
}
#endif
