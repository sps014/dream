#include "dream_core.h"
#include "dream_thread.h"

#include <limits.h>
#include <stdlib.h>

extern dream_ptr dream_worker_invoke(int32_t fn, dream_ptr env, dream_ptr arg);
static dream_ptr worker_recv_blocking(int32_t id);
static dream_mutex reg_mu = DREAM_MUTEX_INIT;

#define worker_failure(message) DREAM_PANIC_LITERAL(u##message)
#define registry_failure(message) do { \
    dream_mutex_unlock(&reg_mu); \
    worker_failure(message); \
} while (0)
#define uthash_fatal(msg) registry_failure("panic: out of memory indexing workers")
#include "uthash.h"

typedef struct Job {
    int32_t fn;
    dream_ptr env;
    dream_ptr msg;
    dream_ptr reply;
    unsigned refs;
    int synchronous;
    int done;
    struct Job *next;
} Job;

typedef struct Worker {
    int32_t id;
    int32_t fn;
    dream_ptr env;
    unsigned refs;
    dream_thread_id thread_id;
    dream_mutex mu;
    dream_cond cv;
    Job *head;
    Job *tail;
    Job *replies;
    Job *reply_tail;
    int dead;
    int busy;
    int detached;
    /* A session holds a pool thread only while it has work or thread-bound callbacks, so
     * stopping an idle session never waits on a wake-up and join. */
    int bound;
    UT_hash_handle hh;
} Worker;

/* Pool threads outlive sessions: thread creation dominated short Task fan-outs, so a thread
 * whose session goes idle parks here for the next session with work instead of exiting. */
typedef struct PoolThread {
    dream_cond cv;
    struct Worker *assigned;
    struct PoolThread *next;
} PoolThread;

#define POOL_IDLE_MAX 64u

static Worker *workers;
static int64_t next_id = 1;
static dream_mutex pool_mu = DREAM_MUTEX_INIT;
static PoolThread *idle_threads;
static unsigned idle_count;
static void destroy_worker(Worker *w);

static void release_worker(Worker *w) {
    if (__atomic_fetch_sub(&w->refs, 1u, __ATOMIC_ACQ_REL) == 1u) {
        destroy_worker(w);
    }
}

static void release_job(Job *j) {
    if (__atomic_fetch_sub(&j->refs, 1u, __ATOMIC_ACQ_REL) == 1u) {
        dream_release(j->msg);
        dream_release_closure_env(j->env);
        dream_release(j->reply);
        free(j);
    }
}

static void release_jobs(Job *j) {
    while (j) {
        Job *next = j->next;
        release_job(j);
        j = next;
    }
}

static Worker *find_worker(int32_t id) {
    Worker *w;
    HASH_FIND(hh, workers, &id, sizeof(id), w);
    return w;
}

static Worker *acquire_worker(int32_t id) {
    dream_mutex_lock(&reg_mu);
    Worker *w = find_worker(id);
    if (w) {
        __atomic_fetch_add(&w->refs, 1u, __ATOMIC_RELAXED);
    }
    dream_mutex_unlock(&reg_mu);
#ifdef DREAM_WORKER_TEST_ACQUIRED
    DREAM_WORKER_TEST_ACQUIRED(w);
#endif
    return w;
}

static void callback_wake(void *context) {
    Worker *w = context;
    dream_mutex_lock(&w->mu);
    dream_cond_broadcast(&w->cv);
    dream_mutex_unlock(&w->mu);
}

static void serve_on_this_thread(Worker *w) {
    dream_thread_attach();
    dream_mutex_lock(&w->mu);
    w->thread_id = dream_thread_self();
    dream_mutex_unlock(&w->mu);
    dream_callback_set_waker(callback_wake, w);
}

/* Serves w until it is dead, or idle with nothing tying it to this thread: native callbacks
 * must run on the thread that registered them, so a session holding one keeps its thread. */
static void run_worker(Worker *w) {
    serve_on_this_thread(w);
    for (;;) {
        dream_callback_drain();
        dream_mutex_lock(&w->mu);
        while (!w->head && !w->dead) {
            if (dream_callback_pending()) {
                dream_mutex_unlock(&w->mu);
                dream_callback_drain();
                dream_mutex_lock(&w->mu);
                continue;
            }
            if (!dream_callback_owned()) {
                break;
            }
            dream_cond_wait(&w->cv, &w->mu);
        }
        if (!w->head || w->dead) {
            /* owner_finish waits for outstanding wake callbacks; posts arriving meanwhile
             * still see the session bound, so this thread must pick them up. */
            dream_mutex_unlock(&w->mu);
            dream_callback_owner_finish();
            dream_mutex_lock(&w->mu);
            if (w->head && !w->dead) {
                dream_mutex_unlock(&w->mu);
                serve_on_this_thread(w);
                continue;
            }
            break;
        }
        Job *j = w->head;
        w->head = j->next;
        if (!w->head) {
            w->tail = NULL;
        }
        j->next = NULL;
        w->busy = 1;
        dream_mutex_unlock(&w->mu);
        /* The invoke ABI consumes the wire argument, but borrows the closure environment. */
        dream_ptr r = dream_worker_invoke(j->fn, j->env, j->msg);
        j->msg = 0;
        dream_publish(r);
        dream_mutex_lock(&w->mu);
        j->reply = r;
        j->done = 1;
        w->busy = 0;
        int queued_reply = !j->synchronous && !w->dead;
        if (queued_reply) {
            if (w->reply_tail) {
                w->reply_tail->next = j;
            } else {
                w->replies = j;
            }
            w->reply_tail = j;
        }
        dream_cond_broadcast(&w->cv);
        dream_mutex_unlock(&w->mu);
        if (!queued_reply) {
            release_job(j);
        }
    }
    /* Dropping the reference and clearing `bound` under one lock lets workerTerminate observe
     * both at once; a non-last reference means someone else keeps w alive past the unlock. */
    int last = __atomic_fetch_sub(&w->refs, 1u, __ATOMIC_ACQ_REL) == 1u;
    w->bound = 0;
    dream_cond_broadcast(&w->cv);
    dream_mutex_unlock(&w->mu);
    if (last) {
        destroy_worker(w);
    }
}

static DREAM_THREAD_PROC(worker_main) {
    PoolThread *pt = arg;
    dream_mutex_lock(&pool_mu);
    for (;;) {
        Worker *w = pt->assigned;
        dream_mutex_unlock(&pool_mu);
        run_worker(w);
        dream_mutex_lock(&pool_mu);
        if (idle_count >= POOL_IDLE_MAX) {
            break;
        }
        pt->assigned = NULL;
        pt->next = idle_threads;
        idle_threads = pt;
        idle_count++;
        while (!pt->assigned) {
            dream_cond_wait(&pt->cv, &pool_mu);
        }
    }
    dream_mutex_unlock(&pool_mu);
    dream_cond_destroy(&pt->cv);
    free(pt);
    return 0;
}

static int assign_thread(Worker *w) {
    dream_mutex_lock(&pool_mu);
    PoolThread *pt = idle_threads;
    if (pt) {
        idle_threads = pt->next;
        idle_count--;
        pt->assigned = w;
        dream_cond_signal(&pt->cv);
        dream_mutex_unlock(&pool_mu);
        return 0;
    }
    dream_mutex_unlock(&pool_mu);
    pt = calloc(1, sizeof(*pt));
    if (!pt) {
        return -1;
    }
    dream_cond_init(&pt->cv);
    pt->assigned = w;
    dream_thread th;
    if (dream_thread_start(&th, worker_main, pt) != 0) {
        dream_cond_destroy(&pt->cv);
        free(pt);
        return -1;
    }
    dream_thread_release(th);
    return 0;
}

int32_t workerSpawn(int32_t fn, dream_ptr env) {
    Worker *w = calloc(1, sizeof(*w));
    if (!w) {
        worker_failure("panic: out of memory creating a worker");
    }
    __atomic_store_n(&dream_rt_mt, 1, __ATOMIC_RELEASE);
    w->fn = fn;
    w->env = env;
    w->refs = 1; /* Registry membership; a bound thread adds its own. */
    dream_publish(env);
    dream_retain(env);
    dream_mutex_init(&w->mu);
    dream_cond_init(&w->cv);
    dream_mutex_lock(&reg_mu);
    if (next_id > INT32_MAX) {
        dream_mutex_unlock(&reg_mu);
        destroy_worker(w);
        worker_failure("panic: worker ID space exhausted");
    }
    int32_t id = w->id = (int32_t)next_id++;
    HASH_ADD(hh, workers, id, sizeof(w->id), w);
    dream_mutex_unlock(&reg_mu);
    return id;
}

/* The caller claimed `bound` while enqueueing; the thread owns a reference until it unbinds.
 * A failed start unregisters w first, so the panic leaves no session reachable by ID. */
static void bind_thread(Worker *w) {
    __atomic_fetch_add(&w->refs, 1u, __ATOMIC_RELAXED);
    if (assign_thread(w) != 0) {
        dream_mutex_lock(&reg_mu);
        if (find_worker(w->id) == w) {
            HASH_DEL(workers, w);
        }
        dream_mutex_unlock(&reg_mu);
        worker_failure("panic: could not start a worker thread");
    }
}

int32_t workerPoolSpawn(void) { return workerSpawn(0, 0); }

static Job *new_job(int32_t fn, dream_ptr env, dream_ptr msg, int synchronous) {
    Job *j = calloc(1, sizeof(*j));
    if (!j) {
        worker_failure("panic: out of memory posting a worker job");
    }
    j->fn = fn;
    j->env = env;
    j->msg = msg;
    j->refs = synchronous ? 2u : 1u;
    j->synchronous = synchronous;
    dream_publish(env);
    dream_publish(msg);
    dream_retain(env);
    dream_retain(msg);
    return j;
}

/* Caller holds w->mu; termination and enqueue have one linearization point. `*bind` asks the
 * caller to bind a pool thread once it has unlocked. */
static int enqueue(Worker *w, Job *j, int *bind) {
    *bind = 0;
    if (w->dead) {
        return 0;
    }
    if (w->tail) {
        w->tail->next = j;
    } else {
        w->head = j;
    }
    w->tail = j;
    *bind = !w->bound;
    w->bound = 1;
    dream_cond_broadcast(&w->cv);
    return 1;
}

void workerPost(int32_t id, dream_ptr msg) {
    Worker *w = acquire_worker(id);
    if (!w) {
        return;
    }
    Job *j = new_job(w->fn, w->env, msg, 0);
    int bind;
    dream_mutex_lock(&w->mu);
    int accepted = enqueue(w, j, &bind);
    dream_mutex_unlock(&w->mu);
    if (bind) {
        bind_thread(w);
    }
    if (!accepted) {
        release_job(j);
    }
    release_worker(w);
}

dream_ptr workerPoolDispatch(int32_t id, int32_t fn, dream_ptr env, dream_ptr msg) {
    Worker *w = acquire_worker(id);
    if (!w) {
        return 0;
    }
    Job *j = new_job(fn, env, msg, 1);
    int bind;
    dream_mutex_lock(&w->mu);
    int accepted = enqueue(w, j, &bind);
    if (bind) {
        dream_mutex_unlock(&w->mu);
        bind_thread(w);
        dream_mutex_lock(&w->mu);
    }
    while (accepted && !j->done && !w->dead) {
        dream_cond_wait(&w->cv, &w->mu);
    }
    dream_ptr r = j->done ? j->reply : 0;
    if (j->done) {
        j->reply = 0;
    }
    dream_mutex_unlock(&w->mu);
    if (!accepted) {
        release_job(j);
    }
    release_job(j);
    release_worker(w);
    return r;
}

static dream_ptr worker_recv_blocking(int32_t id) {
    Worker *w = acquire_worker(id);
    if (!w) {
        return 0;
    }
    dream_mutex_lock(&w->mu);
    while (!w->replies && !w->dead) {
        dream_cond_wait(&w->cv, &w->mu);
    }
    Job *j = w->replies;
    dream_ptr r = 0;
    if (j) {
        w->replies = j->next;
        if (!w->replies) {
            w->reply_tail = NULL;
        }
        r = j->reply;
        j->reply = 0;
    }
    dream_mutex_unlock(&w->mu);
    if (j) {
        release_job(j);
    }
    release_worker(w);
    return r;
}

dream_ptr workerRecv(int32_t id) { return worker_recv_blocking(id); }

static Worker *take_worker(int32_t id) {
    dream_mutex_lock(&reg_mu);
    Worker *w = find_worker(id);
    if (w) {
        HASH_DEL(workers, w);
    }
    dream_mutex_unlock(&reg_mu);
    return w;
}

static void destroy_worker(Worker *w) {
    release_jobs(w->head);
    release_jobs(w->replies);
    dream_release_closure_env(w->env);
    dream_mutex_destroy(&w->mu);
    dream_cond_destroy(&w->cv);
    free(w);
}

/* Marks w dead and cancels its queue. A worker stopped mid-job (or by its own thread) is never
 * waited for, even when a later workerTerminate finds it idle. */
static void stop_worker(Worker *w) {
    dream_mutex_lock(&w->mu);
    w->dead = 1;
    Job *cancelled = w->head;
    w->head = w->tail = NULL;
    for (Job *j = cancelled; j; j = j->next) {
        j->done = 1;
    }
    w->detached |= w->busy || (w->bound && dream_thread_id_eq(dream_thread_self(), w->thread_id));
    dream_cond_broadcast(&w->cv);
    dream_mutex_unlock(&w->mu);
    release_jobs(cancelled);
}

/* Stops a worker without waiting; workerTerminate still unregisters and waits for it. A group
 * stops every member first so their shutdowns overlap instead of running one after another. */
void workerShutdown(int32_t id) {
    Worker *w = acquire_worker(id);
    if (!w) {
        return;
    }
    stop_worker(w);
    release_worker(w);
}

void workerTerminate(int32_t id) {
    Worker *w = take_worker(id);
    if (!w) {
        return;
    }
    stop_worker(w);
    dream_mutex_lock(&w->mu);
    while (!w->detached && w->bound) {
        dream_cond_wait(&w->cv, &w->mu);
    }
    dream_mutex_unlock(&w->mu);
    release_worker(w);
}
