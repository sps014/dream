#include "include/dream_rt_native.h"

#include <stdlib.h>
#include <time.h>

#ifndef DREAM_WASM32
#include <pthread.h>
#endif

#define KIND_HOST FUTURE_KIND_HOST
#define KIND_ALL FUTURE_KIND_ALL
#define KIND_ANY FUTURE_KIND_ANY

typedef struct Node {
    dream_ptr f;
    int64_t due;
    struct Node *next;
} Node;

static _Thread_local Node *rq_head;
static _Thread_local Node *rq_tail;
static _Thread_local Node *timer_head;

static int32_t *i32_at(dream_ptr p, int32_t off) {
    return (int32_t *)((char *)dream_p(p) + off);
}

static dream_ptr *ptr_at(dream_ptr p, int32_t off) {
    return (dream_ptr *)((char *)dream_p(p) + off);
}

static dream_ptr arr_get(dream_ptr arr, int32_t i) {
    return ((dream_ptr *)((char *)dream_p(arr) + LEN_PREFIX_SIZE))[i];
}

void *dream_ft_get(int32_t i);
static void combinator_progress(dream_ptr w, dream_ptr child);

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
/* Foreign-thread completion for @async_host hosts. The host thread publishes the result on the
 * future frame (CAS so double/cancelled completes are no-ops), then queues the future itself;
 * the main loop drains this queue and routes the stored waker exactly like an inline
 * completion (plain enqueue, or combinator progress). While foreign work is outstanding and
 * nothing else is ready, dream_run_loop parks on the condvar instead of returning. */
static pthread_mutex_t wake_mu = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t wake_cv = PTHREAD_COND_INITIALIZER;
static Node *fq_head;
static Node *fq_tail;
static int32_t foreign_pending;

void dream_foreign_work_begin(void) {
    pthread_mutex_lock(&wake_mu);
    foreign_pending += 1;
    pthread_mutex_unlock(&wake_mu);
}

void dream_foreign_work_end(void) {
    pthread_mutex_lock(&wake_mu);
    if (foreign_pending > 0) {
        foreign_pending -= 1;
    }
    pthread_mutex_unlock(&wake_mu);
}

static void fq_push_locked(Node *n) {
    if (!fq_tail) {
        fq_head = n;
        fq_tail = n;
    } else {
        fq_tail->next = n;
        fq_tail = n;
    }
}

void dream_complete_foreign(dream_ptr f, dream_ptr res) {
    int32_t expected = 0;
    Node *n = (Node *)calloc(1, sizeof(Node));
    if (!f) {
        free(n);
        return;
    }
    /* Foreign Future/result are published so host pthreads use atomic RC. */
    dream_publish(f);
    dream_publish(res);
    pthread_mutex_lock(&wake_mu);
    /* Publish under the lock so a concurrent dream_await cannot set its waker between our
     * status flip and our waker read (lost-wakeup guard). */
    if (!__atomic_compare_exchange_n(i32_at(f, F_STATUS), &expected, 1, 0,
                                     __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
        pthread_mutex_unlock(&wake_mu);
        free(n);
        return; /* already completed or cancelled */
    }
    ptr_at(f, F_RESULT)[0] = res;
    if (foreign_pending > 0) {
        foreign_pending -= 1;
    }
    if (ptr_at(f, F_WAKER)[0]) {
        n->f = f;
        fq_push_locked(n);
        pthread_cond_signal(&wake_cv);
        n = NULL; /* drained by the loop */
    }
    pthread_mutex_unlock(&wake_mu);
    free(n);
}

static void foreign_drain(void) {
    Node *n;
    for (;;) {
        pthread_mutex_lock(&wake_mu);
        n = fq_head;
        if (n) {
            fq_head = n->next;
            if (!fq_head) {
                fq_tail = NULL;
            }
        }
        pthread_mutex_unlock(&wake_mu);
        if (!n) {
            return;
        }
        {
            dream_ptr f = n->f;
            free(n);
            {
                dream_ptr w = ptr_at(f, F_WAKER)[0];
                if (!w) {
                    continue;
                }
                ptr_at(f, F_WAKER)[0] = 0;
                {
                    int32_t wk = i32_at(w, F_KIND)[0];
                    if (wk == KIND_ALL || wk == KIND_ANY) {
                        combinator_progress(w, f);
                    } else {
                        dream_enqueue(w);
                    }
                }
            }
        }
    }
}
#endif

#ifdef DREAM_WASM32
__attribute__((export_name(DREAM_SYM_NEW_FUTURE)))
#endif
dream_ptr dream_new_future(int32_t size, int32_t poll, int32_t kind) {
    dream_ptr p = dream_malloc_shared(size < (int32_t)F_SLOTS ? (int32_t)F_SLOTS : size, 0);
    memset(dream_p(p), 0, (size_t)(size < (int32_t)F_SLOTS ? (int32_t)F_SLOTS : size));
    i32_at(p, F_POLL)[0] = poll;
    i32_at(p, F_KIND)[0] = kind;
    return p;
}

void dream_enqueue(dream_ptr f) {
    Node *n;
    if (!f || i32_at(f, F_QUEUED)[0]) {
        return;
    }
    i32_at(f, F_QUEUED)[0] = 1;
    n = (Node *)calloc(1, sizeof(Node));
    n->f = f;
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
    dream_async_complete(f, res);
}

void dream_async_complete(dream_ptr f, dream_ptr res) {
    dream_ptr w;
    int32_t wk;
    if (!f || i32_at(f, F_STATUS)[0]) {
        return;
    }
    ptr_at(f, F_RESULT)[0] = res;
    i32_at(f, F_STATUS)[0] = 1;
    w = ptr_at(f, F_WAKER)[0];
    if (!w) {
        return;
    }
    ptr_at(f, F_WAKER)[0] = 0;
    wk = i32_at(w, F_KIND)[0];
    if (wk == KIND_ALL || wk == KIND_ANY) {
        combinator_progress(w, f);
    } else {
        dream_enqueue(w);
    }
}

void dream_cancel(dream_ptr f) {
    Node **link;
    if (!f || i32_at(f, F_STATUS)[0]) {
        return;
    }
    i32_at(f, F_STATUS)[0] = 2;
    ptr_at(f, F_WAKER)[0] = 0;
    for (link = &timer_head; *link; link = &(*link)->next) {
        if ((*link)->f == f) {
            Node *node = *link;
            *link = node->next;
            free(node);
            return;
        }
    }
}

int32_t dream_async_await(dream_ptr future, dream_ptr *dest, int32_t resume_pc) {
    (void)resume_pc;
    if (!future) {
        if (dest) {
            *dest = 0;
        }
        return 1;
    }
    if (i32_at(future, F_STATUS)[0]) {
        if (dest) {
            *dest = ptr_at(future, F_RESULT)[0];
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
#ifndef DREAM_WASM32
    /* Hold the wake lock across the waker handoff and the status recheck so a foreign
     * completion cannot flip status between them (lost-wakeup guard). */
    {
        int32_t done;
        pthread_mutex_lock(&wake_mu);
        ptr_at(child, F_WAKER)[0] = parent;
        done = __atomic_load_n(i32_at(child, F_STATUS), __ATOMIC_ACQUIRE);
        if (done) {
            dream_enqueue(parent);
        } else {
            /* Awaiting a lazy future is what launches it. */
            dream_start(child);
        }
        pthread_mutex_unlock(&wake_mu);
    }
#else
    ptr_at(child, F_WAKER)[0] = parent;
    if (i32_at(child, F_STATUS)[0]) {
        dream_enqueue(parent);
        return;
    }
    /* Awaiting a lazy future is what launches it. */
    dream_start(child);
#endif
}

#ifdef DREAM_WASM32
__attribute__((export_name(DREAM_SYM_RUN_LOOP)))
#endif
void dream_run_loop(void) {
    for (;;) {
#ifndef DREAM_WASM32
        foreign_drain();
#endif
        while (rq_head) {
            Node *n = rq_head;
            dream_ptr f = n->f;
            rq_head = n->next;
            if (!rq_head) {
                rq_tail = NULL;
            }
            free(n);
            i32_at(f, F_QUEUED)[0] = 0;
            if (i32_at(f, F_STATUS)[0]) {
                continue;
            }
            {
                int32_t poll = i32_at(f, F_POLL)[0];
                if (poll > 0) {
                    ((int32_t (*)(dream_ptr))dream_ft_get(poll))(f);
                }
            }
        }
#ifndef DREAM_WASM32
        foreign_drain();
        /* Drain can enqueue waiters onto rq; run them before parking or returning. */
        if (rq_head) {
            continue;
        }
        if (!timer_head && !fq_head && foreign_pending == 0) {
            return;
        }
#else
        if (!timer_head) {
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
                struct timespec ts;
                int64_t ns = due - now;
                ts.tv_sec = (time_t)(ns / 1000000000LL);
                ts.tv_nsec = (long)(ns % 1000000000LL);
                pthread_mutex_lock(&wake_mu);
                /* Re-check under the lock: a foreign completion between the drain above and
                 * this wait must not be slept through. */
                if (!fq_head) {
                    pthread_cond_timedwait(&wake_cv, &wake_mu, &ts);
                }
                pthread_mutex_unlock(&wake_mu);
            } else if (!timer_head) {
                /* No timers: park until a foreign completion signals. */
                pthread_mutex_lock(&wake_mu);
                if (!fq_head) {
                    pthread_cond_wait(&wake_cv, &wake_mu);
                }
                pthread_mutex_unlock(&wake_mu);
            }
            now = timeNowNanos();
#endif
            while (timer_head && timer_head->due <= now) {
                Node *n = timer_head;
                timer_head = n->next;
                dream_async_complete(n->f, 0);
                free(n);
            }
        }
    }
}

dream_ptr dream_sleep(int32_t ms) {
    dream_ptr f = dream_new_future((int32_t)F_SLOTS, HOST_POLL_INDEX, KIND_HOST);
    Node *n = (Node *)calloc(1, sizeof(Node));
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
    return f;
}

dream_ptr delayMs(int32_t ms) { return dream_sleep(ms); }

static void combinator_progress(dream_ptr w, dream_ptr child) {
    int32_t kind = i32_at(w, F_KIND)[0];
    int32_t rem;
    int32_t n;
    int32_t i;
    dream_ptr kids;
    dream_ptr out;
    if (i32_at(w, F_STATUS)[0]) {
        return;
    }
    if (kind == KIND_ANY) {
        dream_async_complete(w, ptr_at(child, F_RESULT)[0]);
        return;
    }
    rem = i32_at(w, F_REMAINING)[0] - 1;
    i32_at(w, F_REMAINING)[0] = rem;
    if (rem > 0) {
        return;
    }
    n = i32_at(w, F_COUNT)[0];
    kids = ptr_at(w, F_CHILDREN)[0];
    {
        int32_t es = i32_at(w, F_ESIZE)[0];
        if (es <= 0) {
            es = 4;
        }
        out = dream_array_new(n, es);
        for (i = 0; i < n; i++) {
            dream_ptr c = arr_get(kids, i);
            dream_ptr res = c ? ptr_at(c, F_RESULT)[0] : 0;
            memcpy((char *)dream_p(out) + LEN_PREFIX_SIZE + (size_t)i * (size_t)es, &res, (size_t)es);
            dream_release(c);
        }
    }
    dream_async_complete(w, out);
}

static dream_ptr combinator_new(dream_ptr arr, int32_t kind, int32_t esize) {
    int32_t n = arr ? dream_i32(arr)[0] : 0;
    int32_t i;
    dream_ptr w = dream_new_future((int32_t)F_SLOTS, HOST_POLL_INDEX, kind);
    ptr_at(w, F_CHILDREN)[0] = arr;
    i32_at(w, F_COUNT)[0] = n;
    i32_at(w, F_REMAINING)[0] = n;
    i32_at(w, F_ESIZE)[0] = esize > 0 ? esize : 4;
    if (n == 0 && kind == KIND_ALL) {
        dream_async_complete(w, arr);
        return w;
    }
    for (i = 0; i < n; i++) {
        dream_retain(arr_get(arr, i));
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

dream_ptr dream_all(dream_ptr arr, int32_t esize) { return combinator_new(arr, KIND_ALL, esize); }

dream_ptr dream_any(dream_ptr arr) { return combinator_new(arr, KIND_ANY, 8); }
