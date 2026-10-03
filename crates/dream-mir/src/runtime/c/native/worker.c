#include "include/dream_rt_native.h"
#include "include/dream_thread.h"

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
#include "include/uthash.h"

typedef struct Job {
    int32_t fn;
    dream_ptr env;
    dream_ptr msg;
    struct Job *next;
} Job;

typedef struct Worker {
    int32_t id;
    int32_t fn;
    dream_ptr env;
    dream_thread th;
    dream_mutex mu;
    dream_cond cv;
    Job *head;
    Job *tail;
    dream_ptr reply;
    int has_reply;
    int dead;
    int busy;
    int abandoned;
    UT_hash_handle hh;
} Worker;

static Worker *workers;
static int64_t next_id = 1;

static Worker *find_worker(int32_t id);
static void destroy_worker(Worker *w);

static void callback_wake(void *context) {
    Worker *worker = (Worker *)context;
    dream_mutex_lock(&worker->mu);
    dream_cond_signal(&worker->cv);
    dream_mutex_unlock(&worker->mu);
}

static DREAM_THREAD_PROC(worker_main) {
    Worker *w = (Worker *)arg;
    dream_thread_attach();
    for (;;) {
        dream_callback_set_waker(callback_wake, w);
        dream_callback_drain();
        dream_mutex_lock(&w->mu);
        while (!w->head && !w->dead) {
            if (dream_callback_pending()) {
                dream_mutex_unlock(&w->mu);
                dream_callback_drain();
                dream_mutex_lock(&w->mu);
                continue;
            }
            dream_cond_wait(&w->cv, &w->mu);
        }
        if (w->dead && !w->head) {
            int abandoned = w->abandoned;
            dream_mutex_unlock(&w->mu);
            if (abandoned) {
                dream_callback_owner_finish();
                destroy_worker(w);
                return 0;
            }
            break;
        }
        Job *j = w->head;
        w->head = j->next;
        if (!w->head) {
            w->tail = NULL;
        }
        w->busy = 1;
        dream_mutex_unlock(&w->mu);
        dream_ptr r = dream_worker_invoke(j->fn, j->env, j->msg);
        /* Ownership of `j->msg` transferred to `dream_worker_invoke`, which releases it once the
         * body has run; releasing here too over-frees the posted wire string. */
        free(j);
        dream_mutex_lock(&w->mu);
        dream_publish(r);
        w->reply = r;
        w->has_reply = 1;
        w->busy = 0;
        dream_cond_signal(&w->cv);
        dream_mutex_unlock(&w->mu);
    }
    dream_callback_owner_finish();
    return 0;
}

static Worker *find_worker(int32_t id) {
    Worker *w;
    HASH_FIND(hh, workers, &id, sizeof(id), w);
    return w;
}

int32_t workerSpawn(int32_t fn, dream_ptr env) {
    Worker *w = (Worker *)calloc(1, sizeof(Worker));
    if (w == NULL) {
        worker_failure("panic: out of memory creating a worker");
    }
    __atomic_store_n(&dream_rt_mt, 1, __ATOMIC_RELEASE);
    w->fn = fn;
    w->env = (dream_ptr)env;
    dream_publish(w->env);
    dream_retain(w->env);
    dream_mutex_init(&w->mu);
    dream_cond_init(&w->cv);
    dream_mutex_lock(&reg_mu);
    if (next_id > INT32_MAX) {
        dream_mutex_unlock(&reg_mu);
        destroy_worker(w);
        worker_failure("panic: worker ID space exhausted");
    }
    w->id = (int32_t)next_id++;
    HASH_ADD(hh, workers, id, sizeof(w->id), w);
    /* Do not expose a handle until its thread exists; failure must undo registration. */
    if (dream_thread_start(&w->th, worker_main, w) != 0) {
        HASH_DEL(workers, w);
        dream_mutex_unlock(&reg_mu);
        destroy_worker(w);
        worker_failure("panic: could not start a worker thread");
    }
    dream_mutex_unlock(&reg_mu);
    return w->id;
}

int32_t workerPoolSpawn(void) { return workerSpawn(0, 0); }

void workerPost(int32_t id, dream_ptr msg) {
    Worker *w;
    Job *j;
    dream_mutex_lock(&reg_mu);
    w = find_worker(id);
    dream_mutex_unlock(&reg_mu);
    if (!w) {
        return;
    }
    j = (Job *)calloc(1, sizeof(Job));
    if (j == NULL) {
        worker_failure("panic: out of memory posting a worker job");
    }
    j->fn = w->fn;
    j->env = w->env;
    j->msg = msg;
    dream_publish(msg);
    dream_retain(msg);
    dream_mutex_lock(&w->mu);
    if (w->tail) {
        w->tail->next = j;
    } else {
        w->head = j;
    }
    w->tail = j;
    dream_cond_signal(&w->cv);
    dream_mutex_unlock(&w->mu);
}

dream_ptr workerPoolDispatch(int32_t id, int32_t fn, dream_ptr env, dream_ptr msg) {
    Worker *w;
    Job *j;
    dream_mutex_lock(&reg_mu);
    w = find_worker(id);
    dream_mutex_unlock(&reg_mu);
    if (!w) {
        return 0;
    }
    j = (Job *)calloc(1, sizeof(Job));
    if (j == NULL) {
        worker_failure("panic: out of memory dispatching a worker job");
    }
    j->fn = fn;
    j->env = (dream_ptr)env;
    j->msg = msg;
    dream_publish(j->env);
    dream_publish(msg);
    dream_retain(msg);
    dream_mutex_lock(&w->mu);
    if (w->tail) {
        w->tail->next = j;
    } else {
        w->head = j;
    }
    w->tail = j;
    dream_cond_signal(&w->cv);
    dream_mutex_unlock(&w->mu);
    return worker_recv_blocking(id);
}

static dream_ptr worker_recv_blocking(int32_t id) {
    Worker *w;
    dream_ptr r;
    dream_mutex_lock(&reg_mu);
    w = find_worker(id);
    dream_mutex_unlock(&reg_mu);
    if (!w) {
        return 0;
    }
    dream_mutex_lock(&w->mu);
    while (!w->has_reply && !w->dead) {
        dream_cond_wait(&w->cv, &w->mu);
    }
    r = w->reply;
    w->reply = 0;
    w->has_reply = 0;
    dream_mutex_unlock(&w->mu);
    return r;
}

dream_ptr workerRecv(int32_t id) { return worker_recv_blocking(id); }

static Worker *take_worker(int32_t id) {
    dream_mutex_lock(&reg_mu);
    Worker *w = find_worker(id);
    if (w != NULL) {
        HASH_DEL(workers, w);
    }
    dream_mutex_unlock(&reg_mu);
    return w;
}

static void destroy_worker(Worker *w) {
    Job *j;
    if (!w) {
        return;
    }
    j = w->head;
    while (j) {
        Job *next = j->next;
        dream_release(j->msg);
        free(j);
        j = next;
    }
    if (w->reply) {
        dream_release(w->reply);
    }
    dream_release_closure_env(w->env);
    dream_mutex_destroy(&w->mu);
    dream_cond_destroy(&w->cv);
    free(w);
}

void workerTerminate(int32_t id) {
    Worker *w;
    int busy;
    w = take_worker(id);
    if (!w) {
        return;
    }
    dream_mutex_lock(&w->mu);
    w->dead = 1;
    busy = w->busy;
    if (busy) {
        w->abandoned = 1;
    }
    dream_cond_signal(&w->cv);
    dream_mutex_unlock(&w->mu);
    if (busy) {
        /* Hard abort: the body is still running (e.g. Promise.cancel on a tight loop).
         * Detach so we do not hang `del()`; the thread self-frees if it ever exits. */
        dream_thread_detach(w->th);
        return;
    }
    dream_thread_join(w->th);
    destroy_worker(w);
}
