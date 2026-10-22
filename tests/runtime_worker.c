/* Assertions here carry the calls under test, so they must survive optimized builds. */
#undef NDEBUG
#include "../crates/dream-mir/src/runtime/c/sys/native/include/dream_thread.h"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
typedef unsigned(__stdcall *thread_body)(void *);
#else
typedef void *(*thread_body)(void *);
#endif

typedef struct TestCompletion {
    dream_mutex mu;
    dream_cond cv;
    int done;
    uintptr_t reply;
} TestCompletion;

static int fail_start;
static int start_worker(dream_thread *thread, thread_body body, void *arg) {
    return fail_start ? -1 : dream_thread_start(thread, body, arg);
}

#define DREAM_WORKER_TEST_ACQUIRED(w) pause_acquired(w)
static void pause_acquired(void *worker);
#define dream_thread_start start_worker
#include "../crates/dream-mir/src/runtime/c/sys/native/worker.c"
#undef dream_thread_start

static uint32_t live_workers(void) {
    dream_mutex_lock(&reg_mu);
    uint32_t live = slot_count - free_len;
    dream_mutex_unlock(&reg_mu);
    return live;
}

void dream_complete_foreign_owned(dream_ptr future, dream_ptr reply) {
    if (!future) { dream_release(reply); return; }
    TestCompletion *completion = (TestCompletion *)future;
    dream_mutex_lock(&completion->mu);
    assert(!completion->done);
    completion->reply = (uintptr_t)reply;
    completion->done = 1;
    dream_cond_signal(&completion->cv);
    dream_mutex_unlock(&completion->mu);
}

static dream_ptr completion_wait(TestCompletion *completion) {
    dream_mutex_lock(&completion->mu);
    while (!completion->done) { dream_cond_wait(&completion->cv, &completion->mu); }
    dream_ptr reply = (dream_ptr)completion->reply;
    dream_mutex_unlock(&completion->mu);
    dream_cond_destroy(&completion->cv);
    dream_mutex_destroy(&completion->mu);
    return reply;
}

static void completion_init(TestCompletion *completion) {
    memset(completion, 0, sizeof(*completion));
    dream_mutex_init(&completion->mu);
    dream_cond_init(&completion->cv);
}

static void post_job(TestCompletion *completion, int32_t id, int32_t fn, dream_ptr env, dream_ptr message) {
    completion_init(completion);
    assert(workerDispatchAsync((dream_ptr)completion, id, fn, env, message) == 1);
}

static dream_ptr dispatch_reply(int32_t id, int32_t fn, dream_ptr env, dream_ptr message) {
    TestCompletion completion;
    completion_init(&completion);
    assert(workerDispatchAsync((dream_ptr)&completion, id, fn, env, message) == 1);
    return completion_wait(&completion);
}

void dream_future_fini(dream_ptr ptr) { (void)ptr; }
void dream_thread_attach(void) {}
void dream_callback_drain(void) {}
void dream_callback_owner_finish(void) {}
int dream_callback_pending(void) { return 0; }
int dream_callback_owned(void) { return 0; }
int dream_async_pending(void) { return 0; }
void dream_run_loop(void) {}
void dream_callback_set_waker(void (*wake)(void *), void *context) {
    (void)wake;
    (void)context;
}
void dream_release_closure_env(dream_ptr env) { dream_release(env); }
void dream_panic(dream_ptr message) {
    assert(live_workers() == 0);
    for (int i = 0; i < dream_str_len(message); ++i) {
        fputc(dream_char_at_u(message, i), stderr);
    }
    fputc('\n', stderr);
    exit(86);
}

static dream_mutex test_mu = DREAM_MUTEX_INIT;
static dream_cond test_cv = DREAM_COND_INIT;
static int entered, proceed;
static int32_t self_id;

static int32_t paused_id;
static void pause_acquired(void *worker) {
    Worker *w = worker;
    dream_mutex_lock(&test_mu);
    if (w && paused_id == w->id) {
        entered = 1; dream_cond_broadcast(&test_cv);
        while (!proceed) { dream_cond_wait(&test_cv, &test_mu); }
    }
    dream_mutex_unlock(&test_mu);
}

dream_ptr dream_worker_invoke(int32_t fn, dream_ptr env, dream_ptr arg) {
    if (fn == 8) {
        dream_mutex_lock(&test_mu);
        entered = 1;
        dream_cond_broadcast(&test_cv);
        while (!proceed) { dream_cond_wait(&test_cv, &test_mu); }
        dream_mutex_unlock(&test_mu);
    } else if (fn == 9) {
        workerTerminate(self_id);
    } else { assert(fn == 7); }
    if (env) { assert(dream_str_len(env) == 3); }
    return arg;
}

static DREAM_THREAD_PROC(spawn_group) {
    int32_t *ids = arg;
    for (int i = 0; i < 25; ++i) { ids[i] = workerSpawn(0); }
    return 0;
}

typedef struct {
    int32_t id;
    int number;
} Operation;

static DREAM_THREAD_PROC(receive_until_cancelled) {
    Operation *op = arg;
    TestCompletion completion;
    post_job(&completion, op->id, 8, 0, 0);
    assert(completion_wait(&completion) == 0);
    return 0;
}

static DREAM_THREAD_PROC(dispatch_message) {
    Operation *op = arg;
    char text[32];
    int len = snprintf(text, sizeof(text), "dispatch-%d", op->number);
    dream_ptr msg = dream_utf8_to_string(text);
    dream_ptr r = dispatch_reply(op->id, 7, 0, msg);
    assert(r == msg);
    assert(dream_str_len(r) == len);
    dream_release(msg);
    dream_release(r);
    return 0;
}

static void concurrent_operations(void) {
    int32_t id = workerSpawn(0);
    Worker *pin = acquire_worker(id);
    Operation ops[8];
    dream_thread threads[8];
    for (int i = 0; i < 8; ++i) {
        ops[i] = (Operation){id, i};
        assert(dream_thread_start(&threads[i], dispatch_message, &ops[i]) == 0);
    }
    for (int i = 0; i < 8; ++i) { dream_thread_join(threads[i]); }
    workerTerminate(id);
    workerTerminate(id);
    assert(pin->dead);
    assert(__atomic_load_n(&pin->refs, __ATOMIC_ACQUIRE) == 1);
    release_worker(pin);

    id = workerSpawn(0);
    pin = acquire_worker(id);
    entered = proceed = 0;
    ops[0].id = id;
    assert(dream_thread_start(&threads[0], receive_until_cancelled, &ops[0]) == 0);
    /* Termination settles the active wait without waiting for the busy body. */
    for (;;) {
        dream_mutex_lock(&pin->mu);
        int pending = pin->active_future != 0;
        dream_mutex_unlock(&pin->mu);
        if (pending) { break; }
        dream_thread_yield();
    }
    workerTerminate(id);
    dream_thread_join(threads[0]);
    dream_mutex_lock(&test_mu);
    proceed = 1; dream_cond_broadcast(&test_cv);
    dream_mutex_unlock(&test_mu);
    while (__atomic_load_n(&pin->refs, __ATOMIC_ACQUIRE) != 1) { dream_thread_yield(); }
    release_worker(pin);
    assert(debug_get_live_objects() == 0);
}

static DREAM_THREAD_PROC(repeated_sessions) {
    (void)arg;
    for (int i = 0; i < 100; ++i) {
        int32_t id = workerSpawn(1);
        Worker *pin = acquire_worker(id);
        dream_ptr message = dream_utf8_to_string("handoff");
        TestCompletion completion;
        post_job(&completion, id, 7, 0, message);
        dream_ptr reply = completion_wait(&completion);
        assert(reply == message);
        dream_mutex_lock(&pin->mu);
        assert(!pin->bound && !pin->busy && pin->dead);
        dream_mutex_unlock(&pin->mu);
        dream_release(reply);
        dream_release(message);
        workerTerminate(id);
        release_worker(pin);
    }
    return 0;
}

static void concurrent_session_handoffs(void) {
    dream_thread threads[8];
    for (int i = 0; i < 8; ++i) {
        assert(dream_thread_start(&threads[i], repeated_sessions, NULL) == 0);
    }
    for (int i = 0; i < 8; ++i) { dream_thread_join(threads[i]); }
    assert(live_workers() == 0);
    assert(debug_get_live_objects() == 0);
}

static void one_shot_cancellation(void) {
    entered = proceed = 0;
    int32_t id = workerSpawn(1);
    Worker *pin = acquire_worker(id);
    TestCompletion active, rejected;
    post_job(&active, id, 8, 0, 0);
    dream_mutex_lock(&test_mu);
    while (!entered) { dream_cond_wait(&test_cv, &test_mu); }
    dream_mutex_unlock(&test_mu);
    post_job(&rejected, id, 7, 0, 0);
    assert(completion_wait(&rejected) == 0);
    workerTerminate(id);
    assert(completion_wait(&active) == 0);
    dream_mutex_lock(&test_mu);
    proceed = 1; dream_cond_broadcast(&test_cv);
    dream_mutex_unlock(&test_mu);
    while (__atomic_load_n(&pin->refs, __ATOMIC_ACQUIRE) != 1) { dream_thread_yield(); }
    release_worker(pin);
}

static DREAM_THREAD_PROC(post_once) {
    int32_t id = *(int32_t *)arg;
    dream_ptr message = dream_utf8_to_string("acquired");
    TestCompletion completion;
    post_job(&completion, id, 7, 0, message);
    assert(completion_wait(&completion) == 0);
    dream_release(message);
    return 0;
}
static void acquired_operation_survives_termination(void) {
    int32_t id = workerSpawn(0);
    Worker *pin = acquire_worker(id);
    dream_mutex_lock(&test_mu);
    paused_id = id; entered = proceed = 0;
    dream_mutex_unlock(&test_mu);
    dream_thread posting;
    assert(dream_thread_start(&posting, post_once, &id) == 0);
    dream_mutex_lock(&test_mu);
    while (!entered) { dream_cond_wait(&test_cv, &test_mu); }
    dream_mutex_unlock(&test_mu);
    workerTerminate(id);
    assert(pin->dead && __atomic_load_n(&pin->refs, __ATOMIC_ACQUIRE) == 2);
    dream_mutex_lock(&test_mu);
    paused_id = 0; proceed = 1; dream_cond_broadcast(&test_cv);
    dream_mutex_unlock(&test_mu);
    dream_thread_join(posting);
    assert(__atomic_load_n(&pin->refs, __ATOMIC_ACQUIRE) == 1);
    release_worker(pin);
    assert(debug_get_live_objects() == 0);
}

static void termination_boundaries(void) {
    entered = proceed = 0;
    int32_t id = workerSpawn(0);
    TestCompletion completions[21];
    Worker *pin = acquire_worker(id);
    dream_ptr msg = dream_utf8_to_string("busy");
    post_job(&completions[0], id, 8, 0, msg);
    dream_release(msg);
    dream_mutex_lock(&test_mu);
    while (!entered) { dream_cond_wait(&test_cv, &test_mu); }
    dream_mutex_unlock(&test_mu);
    for (int i = 0; i < 12; ++i) {
        msg = dream_utf8_to_string("queued"); post_job(&completions[i + 1], id, 7, 0, msg); dream_release(msg);
    }
    workerTerminate(id);
    assert(pin->dead && pin->busy && !pin->head);
    msg = dream_utf8_to_string("rejected");
    TestCompletion rejected;
    post_job(&rejected, id, 7, 0, msg);
    assert(completion_wait(&rejected) == 0);
    dream_release(msg);
    dream_mutex_lock(&test_mu);
    proceed = 1;
    dream_cond_broadcast(&test_cv);
    dream_mutex_unlock(&test_mu);
    while (__atomic_load_n(&pin->refs, __ATOMIC_ACQUIRE) != 1) { dream_thread_yield(); }
    release_worker(pin);
    for (int i = 0; i < 13; ++i) { assert(completion_wait(&completions[i]) == 0); }

    self_id = workerSpawn(0);
    pin = acquire_worker(self_id);
    msg = dream_utf8_to_string("self");
    post_job(&completions[0], self_id, 9, 0, msg);
    dream_release(msg);
    while (__atomic_load_n(&pin->refs, __ATOMIC_ACQUIRE) != 1) { dream_thread_yield(); }
    assert(pin->dead);
    release_worker(pin);
    workerTerminate(self_id);
    assert(completion_wait(&completions[0]) == 0);
    assert(debug_get_live_objects() == 0);

    id = workerSpawn(0);
    for (int i = 0; i < 20; ++i) {
        msg = dream_array_new(1, 1);
        *((char *)dream_p(msg) + 4) = (char)i;
        post_job(&completions[i], id, 7, 0, msg);
        dream_release(msg);
    }
    for (int i = 0; i < 20; ++i) {
        dream_ptr reply = completion_wait(&completions[i]);
        assert(*((char *)dream_p(reply) + 4) == i);
        dream_release(reply);
    }
    workerTerminate(id);
    assert(debug_get_live_objects() == 0);
}

static DREAM_THREAD_PROC(watchdog) {
    (void)arg;
    dream_mutex mu = DREAM_MUTEX_INIT;
    dream_cond cv = DREAM_COND_INIT;
    int64_t deadline = dream_monotonic_ns() + 30000000000LL;
    dream_mutex_lock(&mu);
    for (int64_t now = dream_monotonic_ns(); now < deadline; now = dream_monotonic_ns()) {
        dream_cond_wait_ns(&cv, &mu, deadline - now);
    }
    fputs("worker harness timed out\n", stderr);
    abort();
}

typedef struct {
    Worker *worker;
    int slot;
} ReuseOperation;
static DREAM_THREAD_PROC(reuse_jobs) {
    ReuseOperation *operation = arg;
    for (int i = 0; i < 10000; i++) {
        Job *j = new_job(operation->worker, operation->slot, 0, 0);
        assert(j->fn == operation->slot);
        assert(!j->env && !j->msg && !j->future && !j->next);
        if ((i & 7) == 0) { dream_thread_yield(); }
        assert(j->fn == operation->slot);
        release_job(operation->worker, j);
    }
    return 0;
}
static void session_job_reuse(void) {
    int32_t id = workerSpawn(0);
    Worker *w = acquire_worker(id);
    dream_ptr payload = dream_utf8_to_string("reuse");
    Job *original = NULL;
    for (int i = 0; i < 1000; i++) {
        Job *j = new_job(w, 7, payload, payload);
        if (!original) { original = j; }
        assert(j == original && !w->spare && !j->future && !j->next);
        j->next = j;
        release_job(w, j);
        assert(w->spare == original && !j->future && !j->msg && !j->env);
    }
    dream_release(payload);
    assert(debug_get_live_objects() == 0);
    dream_thread threads[8];
    ReuseOperation operations[8];
    for (int i = 0; i < 8; i++) {
        operations[i] = (ReuseOperation){w, i};
        assert(dream_thread_start(&threads[i], reuse_jobs, &operations[i]) == 0);
    }
    for (int i = 0; i < 8; i++) { dream_thread_join(threads[i]); }
    assert(w->spare && !w->spare->future && !w->spare->msg && !w->spare->env);
    workerTerminate(id);
    release_worker(w);
}

/* FIFO reuse visits every free slot once per lap, so 256 laps wrap each 8-bit generation. */
static void generational_ids(void) {
    int32_t first = workerSpawn(0);
    uint32_t slot = (uint32_t)first & WORKER_SLOT_MASK;
    workerTerminate(first);
    assert(!acquire_worker(first));
    int32_t previous = first;
    int saw_wrap = 0;
    int laps = 256 * ((int)slot_count + 1);
    for (int i = 0; i < laps; i++) {
        int32_t id = workerSpawn(0);
        assert(id > 0);
        assert(id != previous);
        if (((uint32_t)id & WORKER_SLOT_MASK) == slot && ((uint32_t)id >> WORKER_SLOT_BITS) == 1u
            && i > 0) {
            saw_wrap = 1;
        }
        assert(!acquire_worker(previous));
        previous = id;
        workerTerminate(id);
    }
    assert(saw_wrap);
    int32_t a = workerSpawn(0), b = workerSpawn(0);
    workerTerminate(a);
    workerTerminate(b);
    int32_t c = workerSpawn(0);
    assert(((uint32_t)c & WORKER_SLOT_MASK) != ((uint32_t)b & WORKER_SLOT_MASK));
    workerTerminate(c);
    assert(live_workers() == 0);
}

int main(int argc, char **argv) {
    dream_thread guard;
    assert(dream_thread_start(&guard, watchdog, NULL) == 0);
    dream_thread_release(guard);
    if (argc > 1) {
        fail_start = strcmp(argv[1], "start-failure") == 0;
        /* Threads bind on the first job, so a start failure surfaces at dispatch. */
        int32_t id = workerSpawn(0);
        dream_ptr msg = dream_utf8_to_string("start");
        TestCompletion completion;
        post_job(&completion, id, 7, 0, msg);
        return 1;
    }

    for (int round = 0; round < 3; ++round) {
        int32_t ids[100];
        TestCompletion completions[100];
        dream_ptr env = dream_utf8_to_string("env");
        for (int i = 0; i < 100; ++i) {
            ids[i] = workerSpawn(0);
            assert(ids[i] > 0);
            for (int j = 0; j < i; ++j) { assert(ids[i] != ids[j]); }
        }
        assert(live_workers() == 100);
        for (int i = 0; i < 100; ++i) {
            char text[16];
            snprintf(text, sizeof(text), "job-%d", i);
            dream_ptr msg = dream_utf8_to_string(text);
            post_job(&completions[i], ids[i], 7, env, msg);
            dream_release(msg);
        }
        dream_release(env);
        for (int i = 0; i < 100; ++i) {
            char text[16];
            int len = snprintf(text, sizeof(text), "job-%d", i);
            dream_ptr reply = completion_wait(&completions[i]);
            assert(dream_str_len(reply) == len);
            for (int j = 0; j < len; ++j) {
                assert(dream_char_at_u(reply, j) == text[j]);
            }
            dream_release(reply);
        }
        for (int i = 99; i >= 0; --i) { workerTerminate(ids[i]); }
        assert(live_workers() == 0);
        assert(debug_get_live_objects() == 0);
    }

    int32_t pool[100];
    dream_thread owners[4];
    for (int i = 0; i < 4; ++i) {
        assert(dream_thread_start(&owners[i], spawn_group, &pool[i * 25]) == 0);
    }
    for (int i = 0; i < 4; ++i) { dream_thread_join(owners[i]); }
    assert(live_workers() == 100);
    for (int i = 0; i < 100; ++i) {
        for (int j = 0; j < i; ++j) { assert(pool[i] != pool[j]); }
    }
    for (int i = 0; i < 100; ++i) {
        dream_ptr msg = dream_utf8_to_string("pool");
        dream_ptr reply = dispatch_reply(pool[i], 7, 0, msg);
        assert(reply == msg);
        dream_release(msg);
        dream_release(reply);
    }
    for (int i = 0; i < 100; ++i) { workerTerminate(pool[i]); }
    assert(live_workers() == 0);
    assert(debug_get_live_objects() == 0);
    session_job_reuse();
    acquired_operation_survives_termination();
    concurrent_operations();
    concurrent_session_handoffs();
    one_shot_cancellation();
    termination_boundaries();
    generational_ids();
    puts("worker registry stress passed");
}

void dream_release_object(dream_ptr ptr) { dream_release(ptr); }
