#include "../crates/dream-mir/src/runtime/c/native/include/dream_rt_native.h"
#include "../crates/dream-mir/src/runtime/c/native/include/dream_thread.h"
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
    int64_t slot;
} Node;

static dream_mutex mutex = DREAM_MUTEX_INIT;
static dream_cond condition = DREAM_COND_INIT;
static int phase;
static int64_t current_slot;

/* These mirror generated typed destruction, including the user's observably-live del(). */
static void node_destroy(dream_ptr ptr) {
    Node *node = (Node *)dream_p(ptr);
    dream_weak_prepare_destroy(ptr);
    dream_rc_revive(ptr);
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
        int64_t slot = current_slot;
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

int main(void) {
    dream_thread thread;
    assert(dream_thread_start(&thread, reader, NULL) == 0);
    for (int round = 0; round < ROUNDS; ++round) {
        dream_ptr ptr = dream_malloc(sizeof(Node), TAG_STRUCT_BASE);
        Node *node = (Node *)dream_p(ptr);
        node->child = dream_utf8_to_string("child");
        node->slot = 0;
        int64_t slot = weakBind(ptr);
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
        assert(weakDead(slot));
        assert(weakLoad(slot) == 0);
        weakReleaseRaw(slot);
    }
    dream_thread_join(thread);
    assert(debug_get_live_objects() == 0);

    dream_ptr pinned = dream_malloc(sizeof(Node), TAG_STRUCT_BASE);
    memset(dream_p(pinned), 0, sizeof(Node));
    dream_pin_immortal(pinned);
    int64_t slot = weakBind(pinned);
    assert(!weakDead(slot));
    assert(weakLoad(slot) == pinned);
    assert(*dream_rc_word(pinned) == DREAM_RC_IMMORTAL);
    weakReleaseRaw(slot);
    assert(debug_get_live_objects() == 0);
    puts("weak lifetime stress passed");
}
