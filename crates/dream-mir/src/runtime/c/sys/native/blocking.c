#include "dream_blocking.h"
#include "dream_thread.h"

#define BLOCKING_WORKERS 4
#define BLOCKING_CAPACITY 128

typedef struct {
    dream_blocking_work work;
    void *context;
} BlockingJob;

static dream_mutex blocking_mu = DREAM_MUTEX_INIT;
static dream_cond blocking_ready = DREAM_COND_INIT;
static BlockingJob blocking_queue[BLOCKING_CAPACITY];
static unsigned blocking_head, blocking_count, blocking_workers;
static int blocking_initialized;

static DREAM_THREAD_PROC(blocking_run) {
    (void)arg;
    for (;;) {
        dream_mutex_lock(&blocking_mu);
        while (!blocking_count) {
            dream_cond_wait(&blocking_ready, &blocking_mu);
        }
        BlockingJob job = blocking_queue[blocking_head];
        blocking_head = (blocking_head + 1) % BLOCKING_CAPACITY;
        --blocking_count;
        dream_mutex_unlock(&blocking_mu);
        job.work(job.context);
    }
    return 0;
}

int dream_blocking_submit(dream_blocking_work work, void *context) {
    if (!work) { return 0; }
    dream_mutex_lock(&blocking_mu);
    if (!blocking_initialized) {
        blocking_initialized = 1;
        for (unsigned i = 0; i < BLOCKING_WORKERS; ++i) {
            dream_thread thread;
            if (dream_thread_start(&thread, blocking_run, NULL)) { break; }
            dream_thread_release(thread);
            ++blocking_workers;
        }
    }
    if (!blocking_workers || blocking_count == BLOCKING_CAPACITY) {
        dream_mutex_unlock(&blocking_mu);
        return 0;
    }
    unsigned tail = (blocking_head + blocking_count) % BLOCKING_CAPACITY;
    blocking_queue[tail] = (BlockingJob){work, context};
    ++blocking_count;
    dream_cond_signal(&blocking_ready);
    dream_mutex_unlock(&blocking_mu);
    return 1;
}
