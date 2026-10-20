#include "dream_core.h"
#include <stdlib.h>
#ifndef DREAM_WASM32
#include "dream_thread.h"
#endif

typedef struct Cancellation Cancellation;
typedef struct Registration {
    /* JS Atomics.waitAsync observes this aligned word, never guest object fields. */
    int32_t done;
    int32_t linked;
    Cancellation *source;
    struct Registration *previous, *next;
    dream_ptr future;
} Registration;
struct Cancellation {
#ifdef DREAM_WASM32
    int32_t lock;
#else
    dream_mutex lock;
#endif
    unsigned references;
    int32_t cancelled;
    Registration *first, *last;
};
#ifdef DREAM_WASM32
static void acquire(Cancellation *s) { dream_lock_acquire((dream_ptr)(uintptr_t)&s->lock); }
static void release(Cancellation *s) { dream_lock_release((dream_ptr)(uintptr_t)&s->lock); }
#else
static void acquire(Cancellation *s) { dream_mutex_lock(&s->lock); }
static void release(Cancellation *s) { dream_mutex_unlock(&s->lock); }
#endif

static void detach(Registration *r) {
    if (!r->linked) { return; }
    Cancellation *s = r->source;
    if (r->previous) { r->previous->next = r->next; } else { s->first = r->next; }
    if (r->next) { r->next->previous = r->previous; } else { s->last = r->previous; }
    r->previous = r->next = NULL;
    r->linked = 0;
}

static void wake(Registration *r, int32_t done) {
#ifdef DREAM_WASM32_THREADS
    __atomic_store_n(&r->done, done, __ATOMIC_RELEASE);
#else
    r->done = done;
#endif
    if (!r->future) { return; }
#if defined(DREAM_WASM32) && !defined(DREAM_COMPONENT)
    cancellation_notify((uintptr_t)r);
#else
    dream_event_complete(r->future);
#endif
    r->future = 0;
}

uintptr_t dream_cancellation_new(void) {
    Cancellation *s = calloc(1, sizeof(*s));
    if (!s) { DREAM_PANIC_LITERAL(u"out of memory creating cancellation source"); }
#ifndef DREAM_WASM32
    dream_mutex_init(&s->lock);
#endif
    s->references = 1;
    return (uintptr_t)s;
}

void dream_cancellation_retain(uintptr_t handle) {
    Cancellation *s = (Cancellation *)handle;
#if !defined(DREAM_WASM32) || defined(DREAM_WASM32_THREADS)
    __atomic_fetch_add(&s->references, 1u, __ATOMIC_RELAXED);
#else
    ++s->references;
#endif
}

void dream_cancellation_release(uintptr_t handle) {
    Cancellation *s = (Cancellation *)handle;
    if (!s) { return; }
    /* Completing a signal can drop the last guest token while the registration lock is held.
     * Its registration reference keeps this state alive; reference release must not relock it. */
#if !defined(DREAM_WASM32) || defined(DREAM_WASM32_THREADS)
    unsigned previous = __atomic_fetch_sub(&s->references, 1u, __ATOMIC_ACQ_REL);
#else
    unsigned previous = s->references--;
#endif
    if (previous == 1) {
#ifndef DREAM_WASM32
        dream_mutex_destroy(&s->lock);
#endif
        free(s);
    }
}

static int32_t cancelled(Cancellation *s) {
#if !defined(DREAM_WASM32) || defined(DREAM_WASM32_THREADS)
    return __atomic_load_n(&s->cancelled, __ATOMIC_ACQUIRE);
#else
    return s->cancelled;
#endif
}

int32_t dream_cancellation_is_cancelled(uintptr_t handle) {
    return cancelled((Cancellation *)handle);
}

void dream_cancellation_cancel(uintptr_t handle) {
    Cancellation *s = (Cancellation *)handle;
    acquire(s);
    if (!cancelled(s)) {
#if !defined(DREAM_WASM32) || defined(DREAM_WASM32_THREADS)
        __atomic_store_n(&s->cancelled, 1, __ATOMIC_RELEASE);
#else
        s->cancelled = 1;
#endif
        while (s->first) { Registration *r = s->first; detach(r); wake(r, 1); }
    }
    release(s);
}

uintptr_t dream_cancellation_register(uintptr_t handle) {
    Cancellation *s = (Cancellation *)handle;
    Registration *r = calloc(1, sizeof(*r));
    if (!r) { DREAM_PANIC_LITERAL(u"out of memory registering cancellation"); }
    acquire(s);
    dream_cancellation_retain(handle);
    r->source = s;
    r->done = cancelled(s);
    if (!r->done) {
        r->previous = s->last;
        if (s->last) { s->last->next = r; } else { s->first = r; }
        s->last = r;
        r->linked = 1;
    }
    release(s);
    return (uintptr_t)r;
}

dream_ptr dream_cancellation_signal(uintptr_t handle) {
    Registration *r = (Registration *)handle;
    Cancellation *s = r->source;
    acquire(s);
    dream_ptr future = dream_event_new();
    if (r->done) {
#ifdef DREAM_WASM32
        dream_async_complete(future, 0);
#else
        dream_event_complete(future);
#endif
    } else {
        r->future = future;
#if defined(DREAM_WASM32) && !defined(DREAM_COMPONENT)
        /* Subscribe before releasing the lock: cancellation cannot race JS registration. */
        cancellation_subscribe(future, handle);
#endif
    }
    release(s);
    return future;
}

void dream_cancellation_unregister(uintptr_t handle) {
    Registration *r = (Registration *)handle;
    if (!r) { return; }
    Cancellation *s = r->source;
    acquire(s);
    detach(r);
    /* Even a cancelled future must settle to release its foreign/JS pin. */
    wake(r, 2);
    release(s);
    free(r);
    dream_cancellation_release((uintptr_t)s);
}

/* Channel readiness completes individual registrations without cancelling their source. */
void dream_cancellation_notify_one(uintptr_t handle) {
    if (!handle) { return; }
    Cancellation *s = (Cancellation *)handle;
    acquire(s);
    Registration *r = s->first;
    if (r) { detach(r); wake(r, 1); }
    release(s);
}

void dream_cancellation_notify_registration(uintptr_t handle) {
    Registration *r = (Registration *)handle;
    Cancellation *s = r->source;
    acquire(s);
    detach(r);
    wake(r, 1);
    release(s);
}
