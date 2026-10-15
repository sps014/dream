#include "dream_core.h"
#include "dream_thread.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct Future { int completed; struct Future *next; } Future;
static dream_mutex futures_mu = DREAM_MUTEX_INIT;
static Future *futures;
static unsigned created, completed;
static uintptr_t release_during_completion;
void dream_panic(dream_ptr message) { (void)message; abort(); }
dream_ptr dream_event_new(void) {
    Future *f = calloc(1, sizeof(*f));
    assert(f);
    dream_mutex_lock(&futures_mu);
    f->next = futures;
    futures = f;
    ++created;
    dream_mutex_unlock(&futures_mu);
    return (dream_ptr)f;
}
void dream_complete_foreign(dream_ptr future, dream_result result) {
    assert(result == 0);
    dream_mutex_lock(&futures_mu);
    assert(!((Future *)future)->completed);
    ((Future *)future)->completed = 1;
    ++completed;
    if (release_during_completion) {
        uintptr_t source = release_during_completion;
        release_during_completion = 0;
        dream_cancellation_release(source);
    }
    dream_mutex_unlock(&futures_mu);
}
void dream_event_complete(dream_ptr future) { dream_complete_foreign(future, 0); }
#include "../crates/dream-mir/src/runtime/c/sys/shared/cancellation.c"

static DREAM_THREAD_PROC(subscribe) {
    uintptr_t source = (uintptr_t)arg;
    for (int i = 0; i < 2000; ++i) {
        uintptr_t registration = dream_cancellation_register(source);
        dream_cancellation_signal(registration);
        if (i % 3 == 0) { dream_thread_yield(); }
        dream_cancellation_unregister(registration);
    }
    return 0;
}
static DREAM_THREAD_PROC(cancel) {
    dream_thread_yield();
    dream_cancellation_cancel((uintptr_t)arg);
    dream_cancellation_cancel((uintptr_t)arg);
    return 0;
}
static void readiness_notifications(void) {
    uintptr_t source = dream_cancellation_new();
    uintptr_t registrations[3];
    Future *signals[3];
    for (int i = 0; i < 3; ++i) {
        registrations[i] = dream_cancellation_register(source);
        signals[i] = (Future *)dream_cancellation_signal(registrations[i]);
    }
    dream_cancellation_notify_one(source);
    assert(signals[0]->completed && !signals[1]->completed && !signals[2]->completed);
    dream_cancellation_notify_registration(registrations[2]);
    assert(!signals[1]->completed && signals[2]->completed);
    assert(!dream_cancellation_is_cancelled(source));
    dream_cancellation_notify_one(source);
    assert(signals[1]->completed);
    for (int i = 0; i < 3; ++i) { dream_cancellation_unregister(registrations[i]); }
    dream_cancellation_release(source);

    source = dream_cancellation_new();
    uintptr_t registration = dream_cancellation_register(source);
    dream_cancellation_signal(registration);
    release_during_completion = source;
    dream_cancellation_notify_one(source);
    assert(!release_during_completion);
    dream_cancellation_unregister(registration);
}

int main(void) {
    readiness_notifications();
    for (int round = 0; round < 20; ++round) {
        uintptr_t source = dream_cancellation_new();
        assert(!dream_cancellation_is_cancelled(source));
        dream_thread workers[4];
        for (int i = 0; i < 3; ++i) { assert(!dream_thread_start(&workers[i], subscribe, (void *)source)); }
        assert(!dream_thread_start(&workers[3], cancel, (void *)source));
        for (int i = 0; i < 4; ++i) { dream_thread_join(workers[i]); }
        assert(dream_cancellation_is_cancelled(source));
        uintptr_t late = dream_cancellation_register(source);
        dream_cancellation_release(source);
        dream_cancellation_signal(late);
        dream_cancellation_unregister(late);
    }
    assert(created == completed);
    while (futures) { Future *next = futures->next; free(futures); futures = next; }
    puts("cancellation registration races passed");
}
