/* Assertions here carry the calls under test, so they must survive optimized builds. */
#undef NDEBUG
#include "../crates/dream-mir/src/runtime/c/sys/native/include/dream_thread.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
typedef unsigned(__stdcall *thread_body)(void *);
#else
typedef void *(*thread_body)(void *);
#endif

static int fail_start;
static int start_worker(dream_thread *thread, thread_body body, void *arg) {
    return fail_start ? -1 : dream_thread_start(thread, body, arg);
}

#define DREAM_WORKER_TEST_ACQUIRED(w) pause_acquired(w)
static void pause_acquired(void *worker);
#define dream_thread_start start_worker
#include "../crates/dream-mir/src/runtime/c/sys/native/worker.c"
#undef dream_thread_start

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
    dream_mutex_lock(&reg_mu);
    assert(HASH_COUNT(workers) == 0);
    dream_mutex_unlock(&reg_mu);
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
    for (int i = 0; i < 25; ++i) { ids[i] = workerPoolSpawn(); }
    return 0;
}

typedef struct {
    int32_t id;
    int number;
} Operation;

static DREAM_THREAD_PROC(receive_until_cancelled) {
    Operation *op = arg;
    assert(workerRecv(op->id) == 0);
    return 0;
}

static DREAM_THREAD_PROC(dispatch_message) {
    Operation *op = arg;
    char text[32];
    int len = snprintf(text, sizeof(text), "dispatch-%d", op->number);
    dream_ptr msg = dream_utf8_to_string(text);
    dream_ptr r = workerPoolDispatch(op->id, 7, 0, msg);
    assert(r == msg);
    assert(dream_str_len(r) == len);
    dream_release(msg);
    dream_release(r);
    return 0;
}

static void concurrent_operations(void) {
    int32_t id = workerPoolSpawn();
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

    id = workerPoolSpawn();
    pin = acquire_worker(id);
    for (int i = 0; i < 8; ++i) {
        ops[i].id = id;
        assert(dream_thread_start(&threads[i], receive_until_cancelled, &ops[i]) == 0);
    }
    /* Each receiver acquires its reference before waiting; no timing-based race trigger. The
     * session never had work, so no thread holds a reference beside the registry and pin. */
    while (__atomic_load_n(&pin->refs, __ATOMIC_ACQUIRE) != 10) { dream_thread_yield(); }
    workerTerminate(id);
    for (int i = 0; i < 8; ++i) { dream_thread_join(threads[i]); }
    release_worker(pin);
    assert(debug_get_live_objects() == 0);
}

static DREAM_THREAD_PROC(post_once) {
    int32_t id = *(int32_t *)arg;
    dream_ptr message = dream_utf8_to_string("acquired");
    workerPost(id, message);
    dream_release(message);
    return 0;
}
static void acquired_operation_survives_termination(void) {
    int32_t id = workerPoolSpawn();
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
    int32_t id = workerSpawn(8, 0);
    Worker *pin = acquire_worker(id);
    dream_ptr msg = dream_utf8_to_string("busy");
    workerPost(id, msg);
    dream_release(msg);
    dream_mutex_lock(&test_mu);
    while (!entered) { dream_cond_wait(&test_cv, &test_mu); }
    dream_mutex_unlock(&test_mu);
    for (int i = 0; i < 12; ++i) {
        msg = dream_utf8_to_string("queued"); workerPost(id, msg); dream_release(msg);
    }
    workerTerminate(id);
    assert(pin->dead && pin->busy && !pin->head);
    msg = dream_utf8_to_string("rejected");
    workerPost(id, msg);
    dream_release(msg);
    dream_mutex_lock(&test_mu);
    proceed = 1;
    dream_cond_broadcast(&test_cv);
    dream_mutex_unlock(&test_mu);
    while (__atomic_load_n(&pin->refs, __ATOMIC_ACQUIRE) != 1) { dream_thread_yield(); }
    release_worker(pin);

    self_id = workerSpawn(9, 0);
    pin = acquire_worker(self_id);
    msg = dream_utf8_to_string("self");
    workerPost(self_id, msg);
    dream_release(msg);
    while (__atomic_load_n(&pin->refs, __ATOMIC_ACQUIRE) != 1) { dream_thread_yield(); }
    assert(pin->dead);
    release_worker(pin);
    workerTerminate(self_id);
    assert(debug_get_live_objects() == 0);

    id = workerSpawn(7, 0);
    for (int i = 0; i < 20; ++i) {
        msg = dream_array_new(1, 1);
        *((char *)dream_p(msg) + 4) = (char)i;
        workerPost(id, msg);
        dream_release(msg);
    }
    for (int i = 0; i < 20; ++i) {
        dream_ptr reply = workerRecv(id);
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

int main(int argc, char **argv) {
    dream_thread guard;
    assert(dream_thread_start(&guard, watchdog, NULL) == 0);
    dream_thread_release(guard);
    if (argc > 1) {
        if (strcmp(argv[1], "start-failure") == 0) { fail_start = 1; }
        else { next_id = (int64_t)INT32_MAX + 1; }
        /* Threads bind on the first job, so a start failure surfaces at the post. */
        int32_t id = workerPoolSpawn();
        dream_ptr msg = dream_utf8_to_string("start");
        workerPost(id, msg);
        return 1;
    }

    for (int round = 0; round < 3; ++round) {
        int32_t ids[100];
        dream_ptr env = dream_utf8_to_string("env");
        for (int i = 0; i < 100; ++i) {
            ids[i] = workerSpawn(7, env);
            assert(ids[i] > 0);
            if (i) { assert(ids[i] > ids[i - 1]); }
        }
        dream_release(env);
        assert(HASH_COUNT(workers) == 100);
        for (int i = 0; i < 100; ++i) {
            char text[16];
            snprintf(text, sizeof(text), "job-%d", i);
            dream_ptr msg = dream_utf8_to_string(text);
            workerPost(ids[i], msg);
            dream_release(msg);
        }
        for (int i = 0; i < 100; ++i) {
            char text[16];
            int len = snprintf(text, sizeof(text), "job-%d", i);
            dream_ptr reply = workerRecv(ids[i]);
            assert(dream_str_len(reply) == len);
            for (int j = 0; j < len; ++j) {
                assert(dream_char_at_u(reply, j) == text[j]);
            }
            dream_release(reply);
        }
        for (int i = 99; i >= 0; --i) { workerTerminate(ids[i]); }
        assert(HASH_COUNT(workers) == 0);
        assert(debug_get_live_objects() == 0);
    }

    int32_t pool[100];
    dream_thread owners[4];
    for (int i = 0; i < 4; ++i) {
        assert(dream_thread_start(&owners[i], spawn_group, &pool[i * 25]) == 0);
    }
    for (int i = 0; i < 4; ++i) { dream_thread_join(owners[i]); }
    assert(HASH_COUNT(workers) == 100);
    for (int i = 0; i < 100; ++i) {
        for (int j = 0; j < i; ++j) { assert(pool[i] != pool[j]); }
    }
    for (int i = 0; i < 100; ++i) {
        dream_ptr msg = dream_utf8_to_string("pool");
        dream_ptr reply = workerPoolDispatch(pool[i], 7, 0, msg);
        assert(reply == msg);
        dream_release(msg);
        dream_release(reply);
    }
    for (int i = 0; i < 100; ++i) { workerTerminate(pool[i]); }
    assert(HASH_COUNT(workers) == 0);
    assert(debug_get_live_objects() == 0);
    acquired_operation_survives_termination();
    concurrent_operations();
    termination_boundaries();
    puts("worker registry stress passed");
}

void dream_release_object(dream_ptr ptr) { dream_release(ptr); }
