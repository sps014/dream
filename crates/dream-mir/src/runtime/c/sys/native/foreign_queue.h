#ifndef DREAM_FOREIGN_QUEUE_H
#define DREAM_FOREIGN_QUEUE_H
/* Foreign-thread completion for @async_host hosts. Each thread's scheduler owns the host work it
 * started: the frame records that scheduler in F_SCHEDULER, and the host thread only publishes
 * the result and hands the frame back. Waking, combinator progress and the scheduler retain stay
 * on the owning thread. A scheduler with work in flight never leaves dream_run_loop, so a frame
 * never names a scheduler whose thread has exited. Queue state and pending credits share its
 * mutex. Claim/publication may take that mutex; draining and parking never take the claim lock.
 * Pending credits remain held until the completion is queued, keeping the TLS scheduler alive. */
typedef struct Scheduler {
    dream_mutex mu;
    dream_cond cv;
    Node *fq_head;
    Node *fq_tail;
    int32_t foreign_pending;
} Scheduler;

static dream_mutex foreign_claim_mu = DREAM_MUTEX_INIT;
static _Thread_local Scheduler sched = {.mu = DREAM_MUTEX_INIT, .cv = DREAM_COND_INIT};

static Scheduler **scheduler_at(dream_ptr f) {
    return (Scheduler **)ptr_at(f, F_SCHEDULER);
}

static void callback_wake(void *context) {
    Scheduler *s = context;
    dream_mutex_lock(&s->mu);
    dream_cond_broadcast(&s->cv);
    dream_mutex_unlock(&s->mu);
}

void dream_foreign_work_begin(dream_ptr f) {
    /* Foreign code may start before returning its deferred flag. Publication must happen on
     * the owner while its private ARC obligations are still exclusive. */
    __atomic_store_n(&dream_rt_mt, 1, __ATOMIC_RELEASE);
    dream_publish(f);
    dream_mutex_lock(&foreign_claim_mu);
    dream_mutex_lock(&sched.mu);
    sched.foreign_pending += 1;
    *scheduler_at(f) = &sched;
    dream_mutex_unlock(&sched.mu);
    dream_mutex_unlock(&foreign_claim_mu);
}

void dream_foreign_work_end(dream_ptr f) {
    dream_mutex_lock(&foreign_claim_mu);
    Scheduler *s = *scheduler_at(f);
    if (s) {
        *scheduler_at(f) = NULL;
        dream_mutex_lock(&s->mu);
        s->foreign_pending -= 1;
        dream_mutex_unlock(&s->mu);
    }
    dream_mutex_unlock(&foreign_claim_mu);
}

/* Claiming F_SCHEDULER makes a second completion a no-op. A cancelled frame is still handed back
 * so its owner drops the scheduler retain the host was relying on. */
static void complete_foreign(dream_ptr f, dream_result res, int owned) {
    Node *n;
    Scheduler *s;
    if (!f) {
        if (owned) { dream_release((dream_ptr)res); }
        return;
    }
    n = foreign_node_take();
    n->f = f;
    dream_mutex_lock(&foreign_claim_mu);
    s = *scheduler_at(f);
    if (!s) {
        dream_mutex_unlock(&foreign_claim_mu);
        foreign_node_put(n);
        if (owned) { dream_release((dream_ptr)res); }
        return;
    }
    *scheduler_at(f) = NULL;
    int discarded = __atomic_load_n(i32_at(f, F_STATUS), __ATOMIC_ACQUIRE) != 0;
    if (!discarded) {
        *result_at(f) = res;
        dream_publish(f);
        __atomic_store_n(i32_at(f, F_STATUS), 1, __ATOMIC_RELEASE);
    }
    dream_mutex_lock(&s->mu);
    s->foreign_pending -= 1;
    if (s->fq_tail) {
        s->fq_tail->next = n;
    } else {
        s->fq_head = n;
    }
    s->fq_tail = n;
    dream_cond_signal(&s->cv);
    dream_mutex_unlock(&s->mu);
    dream_mutex_unlock(&foreign_claim_mu);
    if (owned && discarded) { dream_release((dream_ptr)res); }
}

void dream_complete_foreign(dream_ptr f, dream_result res) { complete_foreign(f, res, 0); }
void dream_complete_foreign_owned(dream_ptr f, dream_ptr res) { complete_foreign(f, (dream_result)res, 1); }

static void foreign_drain(void) {
    for (;;) {
        Node *n;
        dream_mutex_lock(&sched.mu);
        n = sched.fq_head;
        if (n) {
            sched.fq_head = n->next;
            if (!sched.fq_head) {
                sched.fq_tail = NULL;
            }
        }
        dream_mutex_unlock(&sched.mu);
        if (!n) {
            return;
        }
        dream_ptr f = n->f;
        foreign_node_put(n);
        dream_ptr w = ptr_at(f, F_WAKER)[0];
        if (w) {
            ptr_at(f, F_WAKER)[0] = 0;
            int32_t wk = i32_at(w, F_KIND)[0];
            if (wk == FUTURE_KIND_ALL || wk == FUTURE_KIND_ANY) {
                dream_combinator_progress(w, f);
            } else {
                dream_enqueue(w);
            }
        }
        scheduler_drop_start_retain(f);
    }
}

static int foreign_outstanding(void) {
    dream_mutex_lock(&sched.mu);
    int outstanding = sched.fq_head != NULL || sched.foreign_pending != 0;
    dream_mutex_unlock(&sched.mu);
    return outstanding;
}
#endif
