#include "../crates/dream-mir/src/runtime/c/native/include/dream_rt_native.h"
#include "../crates/dream-mir/src/runtime/c/native/include/dream_heap_maps.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>

/* Generated-program hooks: these graphs are explicitly recycled after their worker exits. */
void dream_release_object(dream_ptr ptr) { dream_release(ptr); }
void dream_release_closure_env(dream_ptr ptr) { dream_release(ptr); }
void dream_thread_attach(void) {}
void dream_callback_drain(void) {}
void dream_callback_owner_finish(void) {}
int dream_callback_pending(void) { return 0; }
void dream_callback_set_waker(void (*wake)(void *), void *context) {
    (void)wake;
    (void)context;
}
void dream_future_fini(dream_ptr ptr) {
    (void)ptr;
    assert(!"publication fixture contains no futures");
}
void dream_panic(dream_ptr msg) {
    (void)msg;
    fputs("publication allocation failed\n", stderr);
    abort();
}

typedef struct {
    dream_ptr left;
    dream_ptr right;
} Node;

static dream_ptr *nodes;
static size_t node_count;
static pthread_mutex_t mutation_mu = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t mutation_cv = PTHREAD_COND_INITIALIZER;
static int installed;

dream_ptr dream_worker_invoke(int32_t fn, dream_ptr env, dream_ptr arg) {
    (void)env;
    if (fn == 1) {
        pthread_mutex_lock(&mutation_mu);
        while (!installed) {
            pthread_cond_wait(&mutation_cv, &mutation_mu);
        }
        assert(dream_tag_shared(nodes[1]));
        assert(dream_tag_shared(nodes[2]));
        pthread_mutex_unlock(&mutation_mu);
        for (size_t i = 0; i < 100000; ++i) {
            dream_retain(nodes[1]);
            dream_release(nodes[1]);
        }
        dream_release(arg);
        return 0;
    }
    for (size_t i = 0; i < node_count; ++i) {
        assert(dream_heap_is_live(nodes[i]));
        assert(dream_tag_shared(nodes[i]));
        assert(*dream_rc_word(nodes[i]) < 0);
        dream_retain(nodes[i]);
        dream_release(nodes[i]);
    }
    dream_release(arg);
    return 0;
}

static void make_sized_nodes(size_t count, int32_t size) {
    nodes = (dream_ptr *)calloc(count, sizeof(*nodes));
    assert(nodes != NULL);
    node_count = count;
    for (size_t i = 0; i < count; ++i) {
        nodes[i] = dream_malloc(size, TAG_STRUCT_BASE);
        Node *node = (Node *)dream_p(nodes[i]);
        node->left = 0;
        node->right = 0;
    }
}

static void make_nodes(size_t count) { make_sized_nodes(count, sizeof(Node)); }

static void handoff_and_free(void) {
    int32_t worker = workerSpawn(0, 0);
    workerPost(worker, nodes[0]);
    assert(workerRecv(worker) == 0);
    workerTerminate(worker);
    for (size_t i = 0; i < node_count; ++i) {
        assert((*dream_rc_word(nodes[i]) & INT32_MAX) == 1);
        dream_recycle(nodes[i]);
    }
    free(nodes);
}

int main(void) {
    make_nodes(3);
    dream_publish_child(nodes[0], nodes[1]);
    assert(!dream_tag_shared(nodes[1]));
    dream_publish(nodes[0]);
    ((Node *)dream_p(nodes[1]))->left = nodes[2];
    dream_publish_child(nodes[0], nodes[1]);
    assert(dream_tag_shared(nodes[1]));
    assert(dream_tag_shared(nodes[2]));
    ((Node *)dream_p(nodes[0]))->left = nodes[1];
    dream_publish_child(nodes[0], 0);
    handoff_and_free();

    make_nodes(3);
    int32_t observer = workerSpawn(1, 0);
    workerPost(observer, nodes[0]);
    /* The observer already owns the root: no second handoff can repair the missing barrier. */
    pthread_mutex_lock(&mutation_mu);
    ((Node *)dream_p(nodes[1]))->left = nodes[2];
    dream_publish_child(nodes[0], nodes[1]);
    ((Node *)dream_p(nodes[0]))->left = nodes[1];
    installed = 1;
    pthread_cond_signal(&mutation_cv);
    pthread_mutex_unlock(&mutation_mu);
    for (size_t i = 0; i < 100000; ++i) {
        dream_retain(nodes[1]);
        dream_release(nodes[1]);
    }
    assert(workerRecv(observer) == 0);
    workerTerminate(observer);
    for (size_t i = 0; i < node_count; ++i) {
        assert((*dream_rc_word(nodes[i]) & INT32_MAX) == 1);
        dream_recycle(nodes[i]);
    }
    free(nodes);

    make_nodes(2);
    /* Ref interiors may point into a heap object: no header is available at the ref address. */
    dream_publish_child(0, nodes[1]);
    assert(dream_tag_shared(nodes[1]));
    ((Node *)dream_p(nodes[0]))->left = nodes[1];
    handoff_and_free();

    make_nodes(1000);
    for (size_t i = 0; i < node_count; ++i) {
        ((Node *)dream_p(nodes[i]))->left = nodes[(i + 1) % node_count];
    }
    handoff_and_free();

    make_nodes(10000);
    for (size_t i = 0; i + 1 < node_count; ++i) {
        Node *node = (Node *)dream_p(nodes[i]);
        node->left = nodes[i + 1];
        node->right = nodes[i + 1];
    }
    handoff_and_free();

    make_nodes(1000000);
    for (size_t i = 0; i + 1 < node_count; ++i) {
        ((Node *)dream_p(nodes[i]))->left = nodes[i + 1];
    }
    handoff_and_free();

    make_nodes(2);
    /* A shared root is not evidence that its constructor's child has been published. */
    __atomic_fetch_or(dream_tag_word(nodes[0]), TAG_SHARED, __ATOMIC_RELAXED);
    ((Node *)dream_p(nodes[0]))->left = nodes[1];
    dream_publish(nodes[0]);
    assert(dream_tag_shared(nodes[1]));
    dream_ptr old = nodes[1];
    nodes[1] = dream_malloc(sizeof(Node), TAG_STRUCT_BASE);
    memset(dream_p(nodes[1]), 0, sizeof(Node));
    ((Node *)dream_p(nodes[0]))->left = nodes[1];
    handoff_and_free();
    dream_recycle(old);

    for (int32_t kind = TAG_ARRAY; ; kind = TAG_CLOSURE_ENV) {
        make_nodes(3);
        dream_recycle(nodes[0]);
        nodes[0] = dream_malloc(4 + 2 * sizeof(dream_ptr), kind);
        dream_i32(nodes[0])[0] = 2;
        memcpy((char *)dream_p(nodes[0]) + 4, nodes + 1, 2 * sizeof(dream_ptr));
        handoff_and_free();
        if (kind == TAG_CLOSURE_ENV) {
            break;
        }
    }
    /* Each allocation needs its own >4 MiB map, exceeding both old registry caps. */
    make_sized_nodes(130, (1 << 22) + 16);
    assert(dream_heap_map_contains_locked(dream_p(nodes[0]), (1 << 22) + 16));
    assert(!dream_heap_map_contains_locked(dream_p(nodes[0]), (1 << 22) + 17));
    assert(!dream_heap_map_contains_locked(NULL, 16));
    for (size_t i = 0; i + 1 < node_count; ++i) {
        ((Node *)dream_p(nodes[i]))->left = nodes[i + 1];
    }
    handoff_and_free();
    assert(debug_get_live_objects() == 0);
    debug_dump_live();
    puts("publication stress passed");
}
