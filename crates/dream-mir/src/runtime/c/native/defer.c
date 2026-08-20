#include "include/dream_rt_native.h"

#include <stdlib.h>

#define DREAM_DEFER_CHUNK 256
#define DREAM_DEFER_QUEUE_CAP (256u * 64u)

typedef void (*dream_destroy_fn)(dream_ptr);

typedef struct dream_defer_chunk {
    dream_ptr ptr[DREAM_DEFER_CHUNK];
    dream_destroy_fn fn[DREAM_DEFER_CHUNK];
    uint32_t n;
    struct dream_defer_chunk *next;
} dream_defer_chunk;

_Thread_local int32_t dream_defer_depth;
_Thread_local int32_t dream_defer_busy;

static _Thread_local dream_defer_chunk *defer_head;
static _Thread_local dream_defer_chunk *defer_tail;
static _Thread_local dream_defer_chunk *defer_cache;
static _Thread_local uint32_t defer_head_i;
static _Thread_local uint32_t defer_queued;

static dream_defer_chunk *chunk_new(void) {
    dream_defer_chunk *c;
    if (defer_cache) {
        c = defer_cache;
        defer_cache = NULL;
        c->n = 0;
        c->next = NULL;
        return c;
    }
    c = (dream_defer_chunk *)calloc(1, sizeof(*c));
    return c;
}

void dream_defer_enter(void) {
    dream_defer_depth += 1;
}

int dream_defer_try_enqueue(dream_ptr p, void (*fn)(dream_ptr)) {
    dream_defer_chunk *c;
    if (dream_defer_depth <= 0 || p == 0 || fn == NULL) {
        return 0;
    }
    if (dream_weak_any) {
        dream_weak_clear_all(p);
    }
    if (defer_tail == NULL || defer_tail->n == DREAM_DEFER_CHUNK) {
        c = chunk_new();
        if (c == NULL) {
            fn(p);
            return 1;
        }
        if (defer_tail) {
            defer_tail->next = c;
        } else {
            defer_head = c;
            defer_head_i = 0;
        }
        defer_tail = c;
    }
    c = defer_tail;
    c->ptr[c->n] = p;
    c->fn[c->n] = fn;
    c->n += 1;
    defer_queued += 1;
    return 1;
}

static void drain_one(void) {
    dream_defer_chunk *h;
    dream_ptr p;
    dream_destroy_fn fn;
    if (defer_head == NULL) {
        return;
    }
    h = defer_head;
    p = h->ptr[defer_head_i];
    fn = h->fn[defer_head_i];
    defer_head_i += 1;
    if (defer_queued > 0) {
        defer_queued -= 1;
    }
    if (defer_head_i >= h->n) {
        defer_head = h->next;
        defer_head_i = 0;
        if (defer_head == NULL) {
            defer_tail = NULL;
        }
        h->n = 0;
        h->next = NULL;
        if (defer_cache == NULL) {
            defer_cache = h;
        } else {
            free(h);
        }
    }
    if (fn) {
        dream_defer_busy += 1;
        fn(p);
        dream_defer_busy -= 1;
    }
}

static void drain_to_cap(void) {
    while (defer_queued > DREAM_DEFER_QUEUE_CAP && defer_head != NULL) {
        drain_one();
    }
}

void dream_defer_drain_all(void) {
    while (defer_head != NULL) {
        drain_one();
    }
    defer_queued = 0;
    if (defer_cache) {
        free(defer_cache);
        defer_cache = NULL;
    }
}

void dream_defer_leave(uint32_t q) {
    uint32_t i;
    int last = dream_defer_depth <= 1;
    for (i = 0; i < q && defer_head != NULL; i++) {
        drain_one();
    }
    if (last) {
        if (q != 0) {
            dream_defer_drain_all();
        } else {
            drain_to_cap();
        }
    } else {
        drain_to_cap();
    }
    if (dream_defer_depth > 0) {
        dream_defer_depth -= 1;
    }
}
