#include "../crates/dream-mir/src/runtime/c/core/include/dream_core.h"
#include "../crates/dream-mir/src/runtime/c/sys/native/include/dream_thread.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

#define ROUNDS 10000

void dream_future_fini(dream_ptr ptr) {
    (void)ptr;
    assert(!"weak fixture contains no futures");
}

void dream_panic(dream_ptr message) {
    (void)message;
    fputs("weak runtime panic\n", stderr);
    abort();
}

typedef struct {
    dream_ptr child;
    uintptr_t slot;
    int id;
} Node;
static int destroyed[ROUNDS];

static void node_visit(dream_ptr ptr) { dream_visit_edge(((Node *)dream_p(ptr))->child); }
static const dream_type_info node_info = {node_visit, NULL, NULL, NULL, 0, 0};
const dream_type_info *dream_type_info_for_tag(int32_t tag) {
    return tag == TAG_STRUCT_BASE ? &node_info : dream_builtin_type_info(tag);
}

static dream_mutex mutex = DREAM_MUTEX_INIT;
static dream_cond condition = DREAM_COND_INIT;
static int phase;
static uintptr_t current_slot;

/* A claimed zero closes the weak-load race before finalization can observe the object. */
static void node_destroy(dream_ptr ptr) {
    Node *node = (Node *)dream_p(ptr);
    assert(__atomic_fetch_add(&destroyed[node->id], 1, __ATOMIC_RELAXED) == 0);
    dream_weak_prepare_destroy(ptr);
    assert(dream_rc_count(ptr) == 0);
    assert(weakDead(node->slot));
    assert(weakLoad(node->slot) == 0);
    dream_release(node->child);
    dream_recycle(ptr);
}

static void node_release(dream_ptr ptr) {
    if (dream_rc_last(ptr)) {
        node_destroy(ptr);
    }
}

static DREAM_THREAD_PROC(reader) {
    (void)arg;
    for (int round = 0; round < ROUNDS; ++round) {
        dream_mutex_lock(&mutex);
        while (phase != 1) {
            dream_cond_wait(&condition, &mutex);
        }
        uintptr_t slot = current_slot;
        dream_mutex_unlock(&mutex);
        dream_ptr first = weakLoad(slot);
        assert(first != 0);
        dream_mutex_lock(&mutex);
        phase = 2;
        dream_cond_signal(&condition);
        dream_mutex_unlock(&mutex);
        node_release(first);
        for (int attempt = 0; attempt < 64; ++attempt) {
            dream_ptr value = weakLoad(slot);
            if (value != 0) {
                Node *node = (Node *)dream_p(value);
                assert(dream_char_at_u(node->child, 0) == 'c');
                assert(dream_tag_shared(node->child));
                node_release(value);
            }
            (void)weakDead(slot);
        }
        dream_mutex_lock(&mutex);
        phase = 3;
        dream_cond_signal(&condition);
        dream_mutex_unlock(&mutex);
    }
    return 0;
}

static void pool_growth_and_reuse(void) {
    enum { COUNT = 2500 };
    dream_ptr slots[COUNT];
    for (int round = 0; round < 3; ++round) {
        int64_t allocations = debug_get_total_allocations();
        dream_ptr target = dream_malloc(4, TAG_INT);
        for (int i = 0; i < COUNT; ++i) {
            slots[i] = target;
            dream_weak_register(target, (dream_ptr)&slots[i], 2, 0);
        }
        assert(debug_get_live_objects() == COUNT + 1);
        assert(debug_get_total_allocations() == allocations + COUNT + 1);
        for (int i = 1; i < COUNT; i += 2) {
            dream_weak_unregister(target, (dream_ptr)&slots[i]);
        }
        assert(debug_get_live_objects() == COUNT / 2 + 1);
        dream_release(target);
        for (int i = 0; i < COUNT; ++i) {
            assert(slots[i] == (i & 1 ? target : 0));
        }
        assert(debug_get_live_objects() == 0);
    }
}

int main(void) {
    pool_growth_and_reuse();
    struct {
        int32_t tag;
        dream_ptr payload;
    } optional = {3, NULL};
    dream_ptr target = dream_malloc(4, TAG_INT);
    optional.payload = target;
    dream_weak_register(target, (dream_ptr)&optional, 0, 17);
    dream_release(target);
    assert(optional.tag == 17);
    assert(optional.payload == NULL);
    assert(debug_get_live_objects() == 0);

    dream_thread thread;
    assert(dream_thread_start(&thread, reader, NULL) == 0);
    for (int round = 0; round < ROUNDS; ++round) {
        dream_ptr ptr = dream_malloc(sizeof(Node), TAG_STRUCT_BASE);
        Node *node = (Node *)dream_p(ptr);
        node->id = round;
        node->child = dream_utf8_to_string("child");
        node->slot = 0;
        uintptr_t slot = weakBind(ptr);
        node->slot = slot;
        dream_mutex_lock(&mutex);
        current_slot = slot;
        phase = 1;
        dream_cond_signal(&condition);
        while (phase < 2) {
            dream_cond_wait(&condition, &mutex);
        }
        dream_mutex_unlock(&mutex);
        if ((round & 1) && dream_rc_claim_unique(ptr)) {
            node_destroy(ptr);
        } else {
            node_release(ptr);
        }
        dream_mutex_lock(&mutex);
        while (phase != 3) {
            dream_cond_wait(&condition, &mutex);
        }
        dream_mutex_unlock(&mutex);
        assert(__atomic_load_n(&destroyed[round], __ATOMIC_RELAXED) == 1);
        assert(weakDead(slot));
        assert(weakLoad(slot) == 0);
        weakReleaseRaw(slot);
    }
    dream_thread_join(thread);
    assert(debug_get_live_objects() == 0);

    dream_ptr pinned = dream_malloc(sizeof(Node), TAG_STRUCT_BASE);
    memset(dream_p(pinned), 0, sizeof(Node));
    dream_pin_immortal(pinned);
    uintptr_t slot = weakBind(pinned);
    assert(!weakDead(slot));
    assert(weakLoad(slot) == pinned);
    assert(*dream_rc_word(pinned) == DREAM_RC_IMMORTAL);
    weakReleaseRaw(slot);
    assert(debug_get_live_objects() == 0);
    puts("weak lifetime stress passed");
}

void dream_release_object(dream_ptr ptr) { dream_release(ptr); }
