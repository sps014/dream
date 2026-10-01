#include "../crates/dream-mir/src/runtime/c/native/include/dream_thread.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static int fail_start;
static int start_worker(dream_thread *thread, void *(*body)(void *), void *arg) {
    return fail_start ? -1 : dream_thread_start(thread, body, arg);
}

#define dream_thread_start start_worker
#include "../crates/dream-mir/src/runtime/c/native/worker.c"
#undef dream_thread_start

void dream_future_fini(dream_ptr ptr) { (void)ptr; }
void dream_thread_attach(void) {}
void dream_callback_drain(void) {}
void dream_callback_owner_finish(void) {}
int dream_callback_pending(void) { return 0; }
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

dream_ptr dream_worker_invoke(int32_t fn, dream_ptr env, dream_ptr arg) {
    assert(fn == 7);
    if (env) { assert(dream_str_len(env) == 3); }
    return arg;
}

static DREAM_THREAD_PROC(spawn_group) {
    int32_t *ids = arg;
    for (int i = 0; i < 25; ++i) { ids[i] = workerPoolSpawn(); }
    return 0;
}

int main(int argc, char **argv) {
    alarm(30);
    if (argc > 1) {
        if (strcmp(argv[1], "start-failure") == 0) { fail_start = 1; }
        else { next_id = (int64_t)INT32_MAX + 1; }
        workerPoolSpawn();
        return 1;
    }

    for (int round = 0; round < 3; ++round) {
        int32_t ids[100];
        dream_ptr env = dream_utf8_to_string("env");
        for (int i = 0; i < 100; ++i) {
            ids[i] = workerSpawn(7, (int64_t)env);
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
    puts("worker registry stress passed");
}
