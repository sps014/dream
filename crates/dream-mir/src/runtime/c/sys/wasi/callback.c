#include "dream_rt_wasm32.h"
#include "dream_platform_internal.h"
#include <stdlib.h>

#define uthash_fatal(message) do { \
    dream_platform_current->unlock(DREAM_LOCK_WEAK); \
    DREAM_PANIC_LITERAL(u"panic: out of memory indexing C callbacks"); \
} while (0)
#include "uthash.h"

typedef struct {
    dream_ptr object;
    int32_t owner;
    UT_hash_handle hh;
} Callback;

static Callback *callbacks;

void dream_callback_enter(void) {}
void dream_thread_attach(void) {}
void dream_thread_detach(void) {}

void dream_callback_register(dream_ptr object) {
    Callback *entry = calloc(1, sizeof(*entry));
    if (!entry) { DREAM_PANIC_LITERAL(u"panic: out of memory registering a C callback"); }
    entry->object = object;
    entry->owner = dream_instance_tid();
    dream_platform_current->lock(DREAM_LOCK_WEAK);
    HASH_ADD(hh, callbacks, object, sizeof(object), entry);
    dream_platform_current->unlock(DREAM_LOCK_WEAK);
}

static Callback *find_owned(dream_ptr object) {
    Callback *entry;
    HASH_FIND(hh, callbacks, &object, sizeof(object), entry);
    if (!entry || entry->owner != dream_instance_tid()) {
        dream_platform_current->unlock(DREAM_LOCK_WEAK);
        DREAM_PANIC_LITERAL(u"panic: a NativeCallback must run on the Dream instance that created it");
    }
    return entry;
}

void dream_callback_check(dream_ptr object) {
    dream_platform_current->lock(DREAM_LOCK_WEAK);
    (void)find_owned(object);
    dream_platform_current->unlock(DREAM_LOCK_WEAK);
}

void dream_callback_unregister(dream_ptr object) {
    dream_platform_current->lock(DREAM_LOCK_WEAK);
    Callback *entry = find_owned(object);
    HASH_DEL(callbacks, entry);
    dream_platform_current->unlock(DREAM_LOCK_WEAK);
    free(entry);
}

void dream_callback_retain(dream_ptr object) {
    dream_callback_check(object);
    dream_retain(object);
}

void dream_callback_release(dream_ptr object) {
    dream_callback_check(object);
    dream_release_object(object);
}
