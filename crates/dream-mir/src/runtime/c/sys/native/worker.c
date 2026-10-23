#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
#include "dream_core.h"
#include "dream_thread.h"

#include <limits.h>
#include <stdlib.h>
#if !defined(_WIN32)
#include <sched.h>
#include <unistd.h>
#endif

extern dream_ptr dream_worker_invoke(int32_t fn, dream_ptr env, dream_ptr arg);
static dream_mutex reg_mu = DREAM_MUTEX_INIT;

#define worker_failure(message) DREAM_PANIC_LITERAL(u##message)

typedef struct Job {
    int32_t fn;
    dream_ptr env;
    dream_ptr msg;
    dream_ptr future;
    struct Job *next;
} Job;

typedef struct Worker {
    int32_t id;
    unsigned refs;
    int one_shot;
    dream_thread_id thread_id;
    dream_mutex mu;
    dream_cond cv;
    Job *head;
    Job *tail;
    Job *spare;
    /* The first spare, so a one-shot session's only job costs no allocation of its own. */
    Job inline_job;
    dream_ptr active_future;
    int dead;
    int busy;
    int detached;
    /* The serving thread is blocked on cv waiting for work or callbacks. Producers clear it as
     * they signal, so a burst into one session wakes its thread once. */
    int parked;
    /* A session holds a pool thread only while it has work or thread-bound callbacks, so
     * stopping an idle session never waits on a wake-up and join. */
    int bound;
    /* Bound but waiting in the pool's run queue for a thread; guarded by pool_mu. */
    int queued;
    struct Worker *run_prev;
    struct Worker *run_next;
} Worker;

/* Pool threads outlive sessions: thread creation dominated short Task fan-outs, so a thread
 * whose session goes idle parks here for the next session with work instead of exiting. */
typedef struct PoolThread {
    struct Worker *assigned;
    /* POOL_IDLE until the thread blocks (POOL_PARKED) or is handed work (POOL_ASSIGNED); an
     * assignment only makes the wake syscall when it replaces POOL_PARKED. */
    uint32_t state;
    struct PoolThread *next;
} PoolThread;

enum { POOL_IDLE, POOL_ASSIGNED, POOL_PARKED };

#define POOL_IDLE_MAX 64u
/* A saturated run queue that dequeues nothing for this long admits one more thread, so jobs
 * that spin on each other without an instrumented wait still make progress. */
#define POOL_STARVATION_NS 50000000LL
/* Dream code keeps no thread-local state a later session could observe, but C linked into the
 * program may (errno included), and every Task starts on a thread of its own. Set before the
 * first worker starts. */
static int isolate_threads;

void dream_worker_isolate_threads(void) { isolate_threads = 1; }

/* Generational ids, the layout of dream_host_abi::handles: id = generation << 23 | slot with
 * generation 1..=255, so ids stay positive and a stale id misses instead of naming a newer
 * session. Free slots are reused FIFO, so a generation wraps only after 255 trips through the
 * whole free ring and the id space never runs out. Guarded by reg_mu. */
#define WORKER_SLOT_BITS 23
#define WORKER_SLOT_MASK ((1u << WORKER_SLOT_BITS) - 1u)
typedef struct WorkerSlot { Worker *worker; uint8_t generation; } WorkerSlot;
static WorkerSlot *slots;
static uint32_t slot_count, slot_capacity;
static uint32_t *free_ring;
static uint32_t free_head, free_len;
/* Sessions run on at most `pool_limit` (the usable CPU count) unblocked threads, plus threads the
 * starvation monitor admitted. Below the limit a session gets a thread at once; at the limit it
 * waits in a FIFO run queue that finishing threads drain without parking. A thread blocked in an
 * instrumented wait (scheduler park, lock, semaphore, session stop) gives up its slot, so jobs
 * waiting on queued jobs cannot deadlock the pool. Isolated threads bypass all of this. */
static dream_mutex pool_mu = DREAM_MUTEX_INIT;
static PoolThread *idle_threads;
static unsigned idle_count;
static unsigned pool_running, pool_limit, pool_injected;
static Worker *run_head, *run_tail;
static uint64_t run_dequeues;
static dream_cond monitor_cv = DREAM_COND_INIT;
static int monitor_started, monitor_watching;
static void destroy_worker(Worker *w);

static void release_worker(Worker *w) {
    if (__atomic_fetch_sub(&w->refs, 1u, __ATOMIC_ACQ_REL) == 1u) {
        destroy_worker(w);
    }
}

static void free_job(Worker *w, Job *j) {
    if (j != &w->inline_job) { free(j); }
}

static void release_job(Worker *w, Job *j) {
    if (j->future) { dream_complete_foreign_owned(j->future, 0); j->future = 0; }
    dream_release(j->msg);
    dream_release_closure_env(j->env);
    j->msg = j->env = 0;
    Job *empty = NULL;
    /* Payload finalizers run before the cleared job becomes reusable. The caller or worker
     * thread holds a session reference, so the spare never crosses session lifetimes. */
    if (!__atomic_compare_exchange_n(&w->spare, &empty, j, 0, __ATOMIC_RELEASE, __ATOMIC_RELAXED)) {
        free_job(w, j);
    }
}

static void release_jobs(Worker *w, Job *j) {
    while (j) {
        Job *next = j->next;
        release_job(w, j);
        j = next;
    }
}

static Worker *find_worker(int32_t id) {
    if (id <= 0) { return NULL; }
    uint32_t slot = (uint32_t)id & WORKER_SLOT_MASK;
    if (slot >= slot_count || slots[slot].generation != (uint32_t)id >> WORKER_SLOT_BITS) {
        return NULL;
    }
    return slots[slot].worker;
}

/* Caller holds reg_mu. Returns 0 when the table cannot grow. */
static int32_t register_worker(Worker *w) {
    uint32_t slot;
    if (free_len > 0) {
        slot = free_ring[free_head];
        free_head = (free_head + 1u) % slot_capacity;
        free_len--;
    } else {
        if (slot_count > WORKER_SLOT_MASK) { return 0; }
        if (slot_count == slot_capacity) {
            uint32_t capacity = slot_capacity ? slot_capacity * 2u : 64u;
            WorkerSlot *grown = realloc(slots, capacity * sizeof(*grown));
            if (!grown) { return 0; }
            slots = grown;
            /* The ring is empty here, so it needs no reordering. */
            uint32_t *ring = realloc(free_ring, capacity * sizeof(*ring));
            if (!ring) { return 0; }
            free_ring = ring;
            free_head = 0;
            slot_capacity = capacity;
        }
        slot = slot_count++;
        slots[slot].generation = 1;
    }
    slots[slot].worker = w;
    return (int32_t)(((uint32_t)slots[slot].generation << WORKER_SLOT_BITS) | slot);
}

/* Caller holds reg_mu and has checked that find_worker(id) is the session to drop. */
static void unregister_worker(int32_t id) {
    uint32_t slot = (uint32_t)id & WORKER_SLOT_MASK;
    slots[slot].worker = NULL;
    slots[slot].generation = slots[slot].generation == 255u ? 1u : slots[slot].generation + 1u;
    free_ring[(free_head + free_len) % slot_capacity] = slot;
    free_len++;
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
    if (w->parked) {
        w->parked = 0;
        dream_cond_broadcast(&w->cv);
    }
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
 * must run on the thread that registered them, so a session holding one keeps its thread, and
 * with isolated threads every session keeps the thread its C state lives on. */
typedef struct WorkerCompletion { dream_ptr future, reply; } WorkerCompletion;

static WorkerCompletion run_worker(Worker *w) {
    WorkerCompletion completed = {0};
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
            if (!isolate_threads && !dream_callback_owned()) {
                break;
            }
            w->parked = 1;
            dream_thread_blocking(1);
            dream_cond_wait(&w->cv, &w->mu);
            dream_thread_blocking(0);
            w->parked = 0;
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
        w->active_future = j->future;
        j->future = 0;
        dream_mutex_unlock(&w->mu);
        /* The invoke ABI consumes the wire argument, but borrows the closure environment. */
        dream_ptr r = dream_worker_invoke(j->fn, j->env, j->msg);
        /* Futures the body started without awaiting finish here, as they would at the end of
         * main; the session's teardown would otherwise strand them. The loop installs its own
         * callback waker, so the session takes its back. */
        if (dream_async_pending()) {
            dream_run_loop();
        }
        dream_callback_set_waker(callback_wake, w);
        j->msg = 0;
        dream_publish(r);
        dream_mutex_lock(&w->mu);
        w->busy = 0;
        dream_ptr future = w->active_future;
        w->active_future = 0;
        if (w->one_shot) { w->dead = 1; }
        dream_mutex_unlock(&w->mu);
        release_job(w, j);
        if (w->one_shot) {
            /* A one-shot reply becomes visible only after closure cleanup and thread-bound
             * callbacks settle; its caller never has to park again just to retire the session. */
            completed = (WorkerCompletion){future, r};
        } else {
            dream_complete_foreign_owned(future, r);
        }
    }
    /* Publish the released thread reference with bound, so synchronous termination owns
     * final cleanup. Destruction waits for this mutex's unlock before reclaiming w. */
    int last = __atomic_fetch_sub(&w->refs, 1u, __ATOMIC_ACQ_REL) == 1u;
    w->bound = 0;
    dream_cond_broadcast(&w->cv);
    dream_mutex_unlock(&w->mu);
    if (last) {
        destroy_worker(w);
    }
    return completed;
}

static unsigned pool_cpu_count(void) {
#if defined(_WIN32)
    DWORD_PTR process_mask, system_mask;
    if (GetProcessAffinityMask(GetCurrentProcess(), &process_mask, &system_mask) && process_mask) {
        return (unsigned)__builtin_popcountll((unsigned long long)process_mask);
    }
    DWORD active = GetActiveProcessorCount(ALL_PROCESSOR_GROUPS);
    return active ? (unsigned)active : 1u;
#else
#if defined(__linux__) && defined(CPU_COUNT)
    cpu_set_t set;
    if (sched_getaffinity(0, sizeof(set), &set) == 0 && CPU_COUNT(&set) > 0) {
        return (unsigned)CPU_COUNT(&set);
    }
#endif
    long online = sysconf(_SC_NPROCESSORS_ONLN);
    return online > 0 ? (unsigned)online : 1u;
#endif
}

/* Caller holds pool_mu. */
static unsigned pool_capacity_locked(void) {
    if (!pool_limit) { pool_limit = pool_cpu_count(); }
    return pool_limit + pool_injected;
}

static void run_push_locked(Worker *w) {
    w->queued = 1;
    w->run_next = NULL;
    w->run_prev = run_tail;
    if (run_tail) { run_tail->run_next = w; } else { run_head = w; }
    run_tail = w;
}

static void run_unlink_locked(Worker *w) {
    if (w->run_prev) { w->run_prev->run_next = w->run_next; } else { run_head = w->run_next; }
    if (w->run_next) { w->run_next->run_prev = w->run_prev; } else { run_tail = w->run_prev; }
    w->run_prev = w->run_next = NULL;
    w->queued = 0;
}

static Worker *run_pop_locked(void) {
    Worker *w = run_head;
    if (w) {
        run_unlink_locked(w);
        run_dequeues++;
    }
    return w;
}

static DREAM_THREAD_PROC(worker_main);

/* Starts w on an idle or new thread; the caller has already counted it in pool_running unless
 * threads are isolated. Thread creation never takes pool_mu, so callers may hold it. */
static int start_thread(Worker *w, int idle_ok) {
    PoolThread *pt = idle_ok ? idle_threads : NULL;
    if (pt) {
        idle_threads = pt->next;
        idle_count--;
        /* A removed idle thread is reachable only from here, so the release exchange
         * publishes `assigned` to it. The thread may run and exit before the wake; futex and
         * WaitOnAddress only hash the address, so waking a freed word is harmless. */
        pt->assigned = w;
        if (__atomic_exchange_n(&pt->state, POOL_ASSIGNED, __ATOMIC_RELEASE) == POOL_PARKED) {
            dream_word_wake_one(&pt->state);
        }
        return 0;
    }
    pt = calloc(1, sizeof(*pt));
    if (!pt) {
        return -1;
    }
    pt->assigned = w;
    pt->state = POOL_ASSIGNED;
    dream_thread th;
    if (dream_thread_start(&th, worker_main, pt) != 0) {
        free(pt);
        return -1;
    }
    dream_thread_release(th);
    return 0;
}

/* Caller holds pool_mu and has made room under the capacity. A failed start leaves the session
 * at the queue head for the next finishing thread or monitor pass. */
static void start_queued_locked(void) {
    Worker *w = run_pop_locked();
    if (!w) { return; }
    pool_running++;
    if (start_thread(w, 1) != 0) {
        pool_running--;
        run_dequeues--;
        w->queued = 1;
        w->run_prev = NULL;
        w->run_next = run_head;
        if (run_head) { run_head->run_prev = w; } else { run_tail = w; }
        run_head = w;
    }
}

static int monitor_needed_locked(void) {
    return run_head != NULL && pool_running >= pool_capacity_locked();
}

static DREAM_THREAD_PROC(pool_monitor) {
    (void)arg;
    dream_mutex_lock(&pool_mu);
    for (;;) {
        while (!monitor_needed_locked()) {
            monitor_watching = 0;
            dream_cond_wait(&monitor_cv, &pool_mu);
        }
        monitor_watching = 1;
        uint64_t seen = run_dequeues;
        dream_cond_wait_ns(&monitor_cv, &pool_mu, POOL_STARVATION_NS);
        if (monitor_needed_locked() && run_dequeues == seen) {
            pool_injected++;
            start_queued_locked();
        }
    }
    return 0;
}

/* Caller holds pool_mu after queueing a session at capacity. Only the transition into watching
 * signals, so a saturated burst costs the monitor one wake. Without a monitor thread the pool
 * still drains; it only loses protection against jobs spinning on each other. */
static void monitor_watch_locked(void) {
    if (!monitor_started) {
        dream_thread th;
        if (dream_thread_start(&th, pool_monitor, NULL) == 0) {
            dream_thread_release(th);
            monitor_started = 1;
            monitor_watching = 1;
        }
        return;
    }
    if (!monitor_watching) {
        monitor_watching = 1;
        dream_cond_signal(&monitor_cv);
    }
}

static void pool_blocking(int entering) {
    dream_mutex_lock(&pool_mu);
    if (entering) {
        pool_running--;
        if (run_head && pool_running < pool_capacity_locked()) {
            start_queued_locked();
        }
    } else {
        pool_running++;
    }
    dream_mutex_unlock(&pool_mu);
}

static DREAM_THREAD_PROC(worker_main) {
    PoolThread *pt = arg;
    if (!isolate_threads) {
        dream_thread_set_blocking_hook(pool_blocking);
    }
    for (;;) {
        Worker *w = pt->assigned;
        WorkerCompletion completed = run_worker(w);
        dream_mutex_lock(&pool_mu);
        if (isolate_threads) {
            dream_mutex_unlock(&pool_mu);
            dream_complete_foreign_owned(completed.future, completed.reply);
            break;
        }
        /* A thread over capacity (after a blocked thread resumed) leaves queued work to others. */
        if (run_head && pool_running <= pool_capacity_locked()) {
            pt->assigned = run_pop_locked();
            dream_mutex_unlock(&pool_mu);
            dream_complete_foreign_owned(completed.future, completed.reply);
            continue;
        }
        pool_running--;
        if (!run_head) {
            pool_injected = 0;
        }
        if (idle_count >= POOL_IDLE_MAX) {
            dream_mutex_unlock(&pool_mu);
            dream_complete_foreign_owned(completed.future, completed.reply);
            break;
        }
        pt->assigned = NULL;
        __atomic_store_n(&pt->state, POOL_IDLE, __ATOMIC_RELAXED);
        pt->next = idle_threads;
        idle_threads = pt;
        idle_count++;
        dream_mutex_unlock(&pool_mu);
        /* Make the retired thread available before waking the caller. An immediate next
         * submission can assign it during completion, avoiding a needless park/wake pair. */
        dream_complete_foreign_owned(completed.future, completed.reply);
        uint32_t state = POOL_IDLE;
        if (__atomic_compare_exchange_n(&pt->state, &state, POOL_PARKED, 0, __ATOMIC_ACQUIRE,
                                        __ATOMIC_ACQUIRE)) {
            state = POOL_PARKED;
        }
        while (state != POOL_ASSIGNED) {
            dream_word_wait(&pt->state, POOL_PARKED);
            state = __atomic_load_n(&pt->state, __ATOMIC_ACQUIRE);
        }
    }
    dream_thread_set_blocking_hook(NULL);
    free(pt);
    dream_heap_thread_exit();
    return 0;
}

static int assign_thread(Worker *w) {
    if (isolate_threads) {
        return start_thread(w, 0);
    }
    dream_mutex_lock(&pool_mu);
    if (pool_running >= pool_capacity_locked()) {
        run_push_locked(w);
        monitor_watch_locked();
        dream_mutex_unlock(&pool_mu);
        return 0;
    }
    pool_running++;
    int started = start_thread(w, 1);
    if (started != 0) {
        pool_running--;
        /* Running threads will reach the queue; only an empty pool has to report failure. */
        if (pool_running > 0) {
            run_push_locked(w);
            started = 0;
        }
    }
    dream_mutex_unlock(&pool_mu);
    return started;
}

/* Takes a session that is still waiting for a thread out of the queue, so stopping it never
 * waits behind busy threads. Returns whether the caller now owns the thread reference. */
static int unqueue_worker(Worker *w) {
    dream_mutex_lock(&pool_mu);
    int queued = w->queued;
    if (queued) {
        run_unlink_locked(w);
    }
    dream_mutex_unlock(&pool_mu);
    return queued;
}

int32_t workerSpawn(int32_t one_shot) {
    Worker *w = calloc(1, sizeof(*w));
    if (!w) {
        worker_failure("panic: out of memory creating a worker");
    }
    __atomic_store_n(&dream_rt_mt, 1, __ATOMIC_RELEASE);
    w->one_shot = one_shot;
    w->spare = &w->inline_job;
    w->refs = 1; /* Registry membership; a bound thread adds its own. */
    dream_mutex_init(&w->mu);
    dream_cond_init(&w->cv);
    dream_mutex_lock(&reg_mu);
    int32_t id = w->id = register_worker(w);
    dream_mutex_unlock(&reg_mu);
    if (id == 0) {
        destroy_worker(w);
        worker_failure("panic: out of memory indexing workers");
    }
    return id;
}

/* The caller claimed `bound` while enqueueing; the thread owns a reference until it unbinds.
 * A failed start unregisters w first, so the panic leaves no session reachable by ID. */
static void bind_thread(Worker *w) {
    __atomic_fetch_add(&w->refs, 1u, __ATOMIC_RELAXED);
    if (assign_thread(w) != 0) {
        dream_mutex_lock(&reg_mu);
        if (find_worker(w->id) == w) {
            unregister_worker(w->id);
        }
        dream_mutex_unlock(&reg_mu);
        worker_failure("panic: could not start a worker thread");
    }
}

static Job *new_job(Worker *w, int32_t fn, dream_ptr env, dream_ptr msg) {
    Job *j = __atomic_exchange_n(&w->spare, NULL, __ATOMIC_ACQUIRE);
    if (!j) {
        j = calloc(1, sizeof(*j));
        if (j) { dream_count(DREAM_COUNT_WORKER_JOB_ALLOC, 1); }
    }
    if (!j) {
        worker_failure("panic: out of memory posting a worker job");
    }
    j->fn = fn;
    j->env = env;
    j->msg = msg;
    j->future = 0;
    j->next = NULL;
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
    if (w->dead || (w->one_shot && w->bound)) {
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
    /* Only the serving thread waits for work: terminators wait after `dead` is set, which
     * rejected this enqueue above. */
    if (w->parked) {
        w->parked = 0;
        dream_cond_signal(&w->cv);
    }
    return 1;
}

int32_t workerDispatchAsync(dream_ptr future, int32_t id, int32_t fn, dream_ptr env, dream_ptr msg) {
    Worker *w = acquire_worker(id);
    if (!w) { dream_complete_foreign_owned(future, 0); return 1; }
    Job *j = new_job(w, fn, env, msg);
    j->future = future;
    int bind;
    dream_mutex_lock(&w->mu);
    int accepted = enqueue(w, j, &bind);
    dream_mutex_unlock(&w->mu);
    if (bind) { bind_thread(w); }
    if (!accepted) { release_job(w, j); }
    release_worker(w);
    return 1;
}

static Worker *take_worker(int32_t id) {
    dream_mutex_lock(&reg_mu);
    Worker *w = find_worker(id);
    if (w) {
        unregister_worker(id);
    }
    dream_mutex_unlock(&reg_mu);
    return w;
}

static void destroy_worker(Worker *w) {
    /* The unbinding thread can still be completing pthread_mutex_unlock after releasing
     * its reference. Acquiring the mutex closes that interval before destroying it. */
    dream_mutex_lock(&w->mu);
    dream_mutex_unlock(&w->mu);
    release_jobs(w, w->head);
    Job *spare = __atomic_exchange_n(&w->spare, NULL, __ATOMIC_ACQUIRE);
    if (spare) { free_job(w, spare); }
    dream_mutex_destroy(&w->mu);
    dream_cond_destroy(&w->cv);
    free(w);
}

/* Marks w dead and cancels its queue. A worker stopped mid-job (or by its own thread) is never
 * waited for, even when a later workerTerminate finds it idle. */
static void stop_worker(Worker *w) {
    dream_mutex_lock(&w->mu);
    w->dead = 1;
    dream_ptr receiver = w->active_future;
    w->active_future = 0;
    Job *cancelled = w->head;
    w->head = w->tail = NULL;
    w->detached |= w->busy || (w->bound && dream_thread_id_eq(dream_thread_self(), w->thread_id));
    dream_cond_broadcast(&w->cv);
    dream_mutex_unlock(&w->mu);
    if (receiver) { dream_complete_foreign_owned(receiver, 0); }
    release_jobs(w, cancelled);
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

/* Native sessions share the process, so a job that panics ends it; no session outlives a crash. */
dream_ptr workerFailure(int32_t id) {
    (void)id;
    return dream_utf8_to_string("");
}

void workerTerminate(int32_t id) {
    Worker *w = take_worker(id);
    if (!w) {
        return;
    }
    stop_worker(w);
    if (unqueue_worker(w)) {
        dream_mutex_lock(&w->mu);
        w->bound = 0;
        dream_mutex_unlock(&w->mu);
        release_worker(w);
    }
    dream_mutex_lock(&w->mu);
    if (!w->detached && w->bound) {
        dream_thread_blocking(1);
        while (!w->detached && w->bound) {
            dream_cond_wait(&w->cv, &w->mu);
        }
        dream_thread_blocking(0);
    }
    dream_mutex_unlock(&w->mu);
    release_worker(w);
}
