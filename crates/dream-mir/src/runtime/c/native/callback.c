#include "include/dream_rt_native.h"
#include "include/dream_thread.h"

#include <stdlib.h>

static dream_mutex callback_mu = DREAM_MUTEX_INIT;
static dream_cond callback_cv = DREAM_COND_INIT;
#define uthash_fatal(message) do { \
    dream_mutex_unlock(&callback_mu); \
    DREAM_PANIC_LITERAL(u"panic: out of memory indexing native callbacks"); \
} while (0)
#include "include/uthash.h"

typedef struct CallbackOwner CallbackOwner;
typedef struct Callback Callback;

struct CallbackOwner {
    Callback *head;
    Callback *ready_head;
    Callback *ready_tail;
    int attached;
    void (*wake)(void *);
    void *context;
    uint32_t waking;
};

struct Callback {
    dream_ptr object;
    CallbackOwner *owner;
    uint32_t pending;
    Callback *next;
    Callback **previous;
    Callback *ready_next;
    UT_hash_handle hh;
};

static Callback *callbacks;
static _Thread_local CallbackOwner *current_owner;

static void callback_failure(void) {
    dream_mutex_unlock(&callback_mu);
    DREAM_PANIC_LITERAL(u"panic: C callbacks must run on their Dream owner thread; dream_thread_attach support for foreign callback calls is planned");
}

void dream_thread_attach(void) {
    if (current_owner) {
        return;
    }
    current_owner = (CallbackOwner *)calloc(1, sizeof(*current_owner));
    if (!current_owner) {
        DREAM_PANIC_LITERAL(u"panic: out of memory registering a callback owner thread");
    }
    current_owner->attached = 1;
}

void dream_callback_enter(void) {
    if (!current_owner) {
        DREAM_PANIC_LITERAL(u"panic: C callbacks must run on their Dream owner thread; dream_thread_attach support for foreign callback calls is planned");
    }
}

void dream_callback_register(dream_ptr object) {
    Callback *callback;
    dream_callback_enter();
    callback = (Callback *)calloc(1, sizeof(*callback));
    if (!callback) {
        DREAM_PANIC_LITERAL(u"panic: out of memory registering a native callback");
    }
    callback->object = object;
    callback->owner = current_owner;
    dream_mutex_lock(&callback_mu);
    callback->next = current_owner->head;
    callback->previous = &current_owner->head;
    if (callback->next) {
        callback->next->previous = &callback->next;
    }
    current_owner->head = callback;
    HASH_ADD(hh, callbacks, object, sizeof(object), callback);
    dream_mutex_unlock(&callback_mu);
}

void dream_callback_unregister(dream_ptr object) {
    Callback *callback;
    dream_mutex_lock(&callback_mu);
    HASH_FIND(hh, callbacks, &object, sizeof(object), callback);
    if (!callback || callback->owner != current_owner || callback->pending) {
        callback_failure();
    }
    *callback->previous = callback->next;
    if (callback->next) {
        callback->next->previous = callback->previous;
    }
    HASH_DEL(callbacks, callback);
    dream_mutex_unlock(&callback_mu);
    free(callback);
}

void dream_callback_check(dream_ptr object) {
    Callback *callback;
    dream_mutex_lock(&callback_mu);
    HASH_FIND(hh, callbacks, &object, sizeof(object), callback);
    if (!callback || callback->owner != current_owner) {
        callback_failure();
    }
    dream_mutex_unlock(&callback_mu);
}

void dream_callback_retain(dream_ptr object) {
    dream_callback_check(object);
    dream_retain(object);
}

void dream_callback_release(dream_ptr object) {
    Callback *callback;
    CallbackOwner *owner;
    void (*wake)(void *);
    void *context;
    dream_mutex_lock(&callback_mu);
    HASH_FIND(hh, callbacks, &object, sizeof(object), callback);
    if (!callback) {
        callback_failure();
    }
    if (callback->owner == current_owner) {
        dream_mutex_unlock(&callback_mu);
        dream_release_object(object);
        return;
    }
    if (!callback->owner->attached || callback->pending == UINT32_MAX
        || callback->owner->waking == UINT32_MAX) {
        dream_mutex_unlock(&callback_mu);
        DREAM_PANIC_LITERAL(u"panic: native callback release requires a live Dream owner thread");
    }
    // The foreign handle's existing retain keeps the object alive until the owner consumes it.
    if (!callback->pending) {
        if (callback->owner->ready_tail) {
            callback->owner->ready_tail->ready_next = callback;
        } else {
            callback->owner->ready_head = callback;
        }
        callback->owner->ready_tail = callback;
    }
    callback->pending += 1;
    owner = callback->owner;
    wake = owner->wake;
    context = owner->context;
    owner->waking += 1;
    dream_mutex_unlock(&callback_mu);
    // Locking a scheduler while holding callback_mu would invert its pending-work check.
    if (wake) {
        wake(context);
    }
    dream_mutex_lock(&callback_mu);
    owner->waking -= 1;
    dream_cond_broadcast(&callback_cv);
    dream_mutex_unlock(&callback_mu);
}

int dream_callback_pending(void) {
    int pending;
    if (!current_owner) {
        return 0;
    }
    dream_mutex_lock(&callback_mu);
    pending = current_owner->ready_head != NULL;
    dream_mutex_unlock(&callback_mu);
    return pending;
}

void dream_callback_set_waker(void (*wake)(void *), void *context) {
    dream_callback_enter();
    dream_mutex_lock(&callback_mu);
    current_owner->wake = wake;
    current_owner->context = context;
    dream_mutex_unlock(&callback_mu);
}

void dream_callback_drain(void) {
    if (!current_owner) {
        return;
    }
    for (;;) {
        Callback *callback;
        dream_ptr object = 0;
        uint32_t pending = 0;
        dream_mutex_lock(&callback_mu);
        callback = current_owner->ready_head;
        if (callback) {
            current_owner->ready_head = callback->ready_next;
            if (!current_owner->ready_head) {
                current_owner->ready_tail = NULL;
            }
            callback->ready_next = NULL;
            object = callback->object;
            pending = callback->pending;
            callback->pending = 0;
        }
        dream_mutex_unlock(&callback_mu);
        if (!pending) {
            return;
        }
        // Destructors may unregister callbacks or re-enter the scheduler; never hold its lock.
        while (pending--) {
            dream_release_object(object);
        }
    }
}

void dream_callback_owner_finish(void) {
    CallbackOwner *owner = current_owner;
    if (!owner) {
        return;
    }
    dream_callback_drain();
    dream_defer_drain_all();
    dream_mutex_lock(&callback_mu);
    owner->attached = 0;
    while (owner->waking) {
        dream_cond_wait(&callback_cv, &callback_mu);
    }
    if (owner->head) {
        dream_mutex_unlock(&callback_mu);
        DREAM_PANIC_LITERAL(u"panic: native callbacks outlive their Dream owner thread");
    }
    current_owner = NULL;
    dream_mutex_unlock(&callback_mu);
    free(owner);
}
