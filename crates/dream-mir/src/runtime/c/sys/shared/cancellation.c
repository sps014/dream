#include "dream_core.h"
#include <stdlib.h>
#ifndef DREAM_WASM32
#include "dream_thread.h"
#endif

typedef struct Cancellation Cancellation;
typedef struct Registration {
    /* JS Atomics.waitAsync observes this aligned word, never guest object fields. */
    int32_t done;
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
    Registration *first;
};
#ifdef DREAM_WASM32
__attribute__((import_module("Dream"), import_name("cancellationSubscribe")))
extern void cancellation_subscribe(dream_ptr future, uintptr_t registration);
__attribute__((import_module("Dream"), import_name("cancellationNotify")))
extern void cancellation_notify(uintptr_t registration);
static void acquire(Cancellation *s) { dream_lock_acquire((dream_ptr)(uintptr_t)&s->lock); }
static void release(Cancellation *s) { dream_lock_release((dream_ptr)(uintptr_t)&s->lock); }
#else
static void acquire(Cancellation *s) { dream_mutex_lock(&s->lock); }
static void release(Cancellation *s) { dream_mutex_unlock(&s->lock); }
#endif

static void wake(Registration *r, int32_t done) {
#ifdef DREAM_WASM32_THREADS
    __atomic_store_n(&r->done, done, __ATOMIC_RELEASE);
#else
    r->done = done;
#endif
    if (!r->future) { return; }
#ifdef DREAM_WASM32
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

void dream_cancellation_release(uintptr_t handle) {
    Cancellation *s = (Cancellation *)handle;
    if (!s) { return; }
    acquire(s);
    unsigned remaining = --s->references;
    release(s);
    if (!remaining) {
#ifndef DREAM_WASM32
        dream_mutex_destroy(&s->lock);
#endif
        free(s);
    }
}

int32_t dream_cancellation_is_cancelled(uintptr_t handle) {
    Cancellation *s = (Cancellation *)handle;
    acquire(s);
    int32_t cancelled = s->cancelled;
    release(s);
    return cancelled;
}

void dream_cancellation_cancel(uintptr_t handle) {
    Cancellation *s = (Cancellation *)handle;
    acquire(s);
    if (!s->cancelled) {
        s->cancelled = 1;
        for (Registration *r = s->first; r; r = r->next) { wake(r, 1); }
    }
    release(s);
}

uintptr_t dream_cancellation_register(uintptr_t handle) {
    Cancellation *s = (Cancellation *)handle;
    Registration *r = calloc(1, sizeof(*r));
    if (!r) { DREAM_PANIC_LITERAL(u"out of memory registering cancellation"); }
    acquire(s);
    ++s->references;
    r->source = s;
    r->done = s->cancelled;
    r->next = s->first;
    if (s->first) { s->first->previous = r; }
    s->first = r;
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
#ifdef DREAM_WASM32
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
    if (r->previous) { r->previous->next = r->next; }
    else { s->first = r->next; }
    if (r->next) { r->next->previous = r->previous; }
    /* Even a cancelled future must settle to release its foreign/JS pin. */
    wake(r, 2);
    release(s);
    free(r);
    dream_cancellation_release((uintptr_t)s);
}
