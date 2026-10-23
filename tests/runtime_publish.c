#include "../crates/dream-mir/src/runtime/c/core/include/dream_core.h"
#include "../crates/dream-mir/src/runtime/c/core/include/dream_heap_maps.h"
#include "../crates/dream-mir/src/runtime/c/core/include/dream_platform_internal.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
#include "dream_thread.h"
typedef struct { dream_mutex mu; dream_cond cv; int done; dream_ptr reply; } TestCompletion;
void dream_complete_foreign_owned(dream_ptr future, dream_ptr reply) {
    if (!future) { dream_release(reply); return; }
    TestCompletion *c = (TestCompletion *)future;
    dream_mutex_lock(&c->mu);
    assert(!c->done); c->reply = reply; c->done = 1;
    dream_cond_signal(&c->cv); dream_mutex_unlock(&c->mu);
}
static dream_ptr dispatch_reply(int32_t worker, int32_t fn, dream_ptr message) {
    TestCompletion c = {0}; dream_mutex_init(&c.mu); dream_cond_init(&c.cv);
    assert(workerDispatchAsync((dream_ptr)&c, worker, fn, 0, message) == 1);
    dream_mutex_lock(&c.mu);
    while (!c.done) { dream_cond_wait(&c.cv, &c.mu); }
    dream_mutex_unlock(&c.mu); dream_cond_destroy(&c.cv); dream_mutex_destroy(&c.mu);
    return c.reply;
}

/* Generated-program hooks: these graphs are explicitly recycled after their worker exits. */
void dream_release_object(dream_ptr ptr) { dream_release(ptr); }
void dream_release_closure_env(dream_ptr ptr) { dream_release(ptr); }
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
static dream_ptr nested_publication;
static size_t node_visits;

static void node_visit(dream_ptr p) {
    node_visits++;
    if (nested_publication) {
        dream_ptr nested = nested_publication;
        nested_publication = 0;
        dream_publish(nested);
    }
    Node *n = (Node *)dream_p(p); dream_visit_edge(n->left); dream_visit_edge(n->right);
}
static void array_visit(dream_ptr p) {
    for (int32_t i = 0; i < dream_i32(p)[0]; ++i) {
        dream_ptr child; memcpy(&child, (char *)dream_p(p) + 4 + (size_t)i * sizeof(child), sizeof(child));
        dream_visit_edge(child);
    }
}
static const dream_type_info node_info = {node_visit, NULL, NULL, NULL};
static const dream_type_info array_info = {array_visit, NULL, NULL, NULL};
const dream_type_info *dream_type_info_for_tag(int32_t tag) {
    if (tag == TAG_STRUCT_BASE) { return &node_info; }
    if (tag == TAG_ARRAY || tag == TAG_CLOSURE_ENV) { return &array_info; }
    return dream_builtin_type_info(tag);
}

static dream_ptr *nodes;
static size_t node_count;
static size_t raw_allocations;
static void *counted_allocate(size_t bytes) {
    __atomic_fetch_add(&raw_allocations, 1, __ATOMIC_RELAXED);
    return dream_default_platform.allocate(bytes);
}
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
    int32_t worker = workerSpawn(0);
    assert(dispatch_reply(worker, 0, nodes[0]) == 0);
    workerTerminate(worker);
    for (size_t i = 0; i < node_count; ++i) {
        assert((*dream_rc_word(nodes[i]) & INT32_MAX) == 1);
        dream_recycle(nodes[i]);
    }
    free(nodes);
}

int main(void) {
    dream_platform platform = dream_default_platform;
    platform.allocate = counted_allocate;
    dream_set_platform(&platform);
    make_nodes(3);
    ((Node *)dream_p(nodes[0]))->left = nodes[1];
    ((Node *)dream_p(nodes[0]))->right = nodes[2];
    nested_publication = nodes[2];
    dream_publish(nodes[0]);
    assert(nested_publication == 0);
    assert(dream_tag_shared(nodes[1]) && dream_tag_shared(nodes[2]));
    handoff_and_free();
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
    int32_t observer = workerSpawn(0);
    TestCompletion observation = {0};
    dream_mutex_init(&observation.mu); dream_cond_init(&observation.cv);
    assert(workerDispatchAsync((dream_ptr)&observation, observer, 1, 0, nodes[0]) == 1);
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
    dream_mutex_lock(&observation.mu);
    while (!observation.done) { dream_cond_wait(&observation.cv, &observation.mu); }
    assert(observation.reply == 0);
    dream_mutex_unlock(&observation.mu);
    dream_cond_destroy(&observation.cv); dream_mutex_destroy(&observation.mu);
    workerTerminate(observer);
    for (size_t i = 0; i < node_count; ++i) {
        assert((*dream_rc_word(nodes[i]) & INT32_MAX) == 1);
        dream_recycle(nodes[i]);
    }
    free(nodes);

    make_nodes(4);
    ((Node *)dream_p(nodes[0]))->left = nodes[1];
    ((Node *)dream_p(nodes[0]))->right = nodes[2];
    ((Node *)dream_p(nodes[1]))->left = nodes[3];
    ((Node *)dream_p(nodes[2]))->left = nodes[3];
    ((Node *)dream_p(nodes[3]))->left = nodes[0];
    /* A Task-sized graph publishes without scratch allocations, visiting each node once. */
    size_t small_before = __atomic_load_n(&raw_allocations, __ATOMIC_RELAXED);
    node_visits = 0;
    dream_publish(nodes[0]);
    assert(node_visits == 4);
    assert(__atomic_load_n(&raw_allocations, __ATOMIC_RELAXED) == small_before);
    ((Node *)dream_p(nodes[3]))->left = 0;
    handoff_and_free();

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
    dream_publish(nodes[0]);
    size_t repeated_before = __atomic_load_n(&raw_allocations, __ATOMIC_RELAXED);
    for (size_t i = 0; i < 20; i++) { dream_publish(nodes[0]); }
    size_t repeated_after = __atomic_load_n(&raw_allocations, __ATOMIC_RELAXED);
    assert(repeated_after - repeated_before <= 20 * 8);
    /* Barriers keep a shared object's children shared, so a republish stops below the root. */
    node_visits = 0;
    dream_publish(nodes[0]);
    assert(node_visits == 1);
    handoff_and_free();

    make_nodes(10000);
    for (size_t i = 0; i + 1 < node_count; ++i) {
        Node *node = (Node *)dream_p(nodes[i]);
        node->left = nodes[i + 1];
        node->right = nodes[i + 1];
    }
    size_t before = __atomic_load_n(&raw_allocations, __ATOMIC_RELAXED);
    dream_publish(nodes[0]);
    size_t after = __atomic_load_n(&raw_allocations, __ATOMIC_RELAXED);
    assert(after - before < node_count / 16);
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
    uintptr_t map_base;
    size_t map_len;
    assert(dream_heap_map_contains_locked(dream_p(nodes[0]), (1 << 22) + 16, &map_base, &map_len));
    assert(map_base <= (uintptr_t)dream_p(nodes[0]));
    assert((uintptr_t)dream_p(nodes[0]) + (1 << 22) + 16 <= map_base + map_len);
    assert(!dream_heap_map_contains_locked(dream_p(nodes[0]), (1 << 22) + 17, &map_base, &map_len));
    assert(!dream_heap_map_contains_locked(NULL, 16, &map_base, &map_len));
    for (size_t i = 0; i + 1 < node_count; ++i) {
        ((Node *)dream_p(nodes[i]))->left = nodes[i + 1];
    }
    handoff_and_free();
    assert(debug_get_live_objects() == 0);
    debug_dump_live();
    puts("publication stress passed");
}
