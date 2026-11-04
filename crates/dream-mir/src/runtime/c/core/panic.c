#include "dream_core.h"
#include "dream_platform_internal.h"

#ifndef __wasm__
#include <stdio.h>
#endif

#define PANIC_MESSAGE_MAX 1024

static dream_panic_hook panic_hook;
/* A hook that itself panics must not recurse into the hook again. */
static _Thread_local int in_panic_hook;

void dream_set_panic_hook(dream_panic_hook hook) {
    __atomic_store_n(&panic_hook, hook, __ATOMIC_RELEASE);
}

void dream_panic_at(dream_ptr msg, const char *location) {
    dream_panic_hook hook = __atomic_load_n(&panic_hook, __ATOMIC_ACQUIRE);
    if (hook && !in_panic_hook) {
        char message[PANIC_MESSAGE_MAX];
        size_t written;
        dream_utf8_chunk(dream_str_units(msg), dream_str_len(msg), message, sizeof(message), 0, &written, 0);
        in_panic_hook = 1;
        hook(message, location);
    } else {
        dream_platform_current->write(2, dream_str_units(msg), (size_t)dream_str_len(msg), DREAM_TEXT_UTF16);
        dream_platform_current->write(2, "\n", 1, DREAM_TEXT_UTF8);
        if (location) {
            dream_platform_current->write(2, "  at ", 5, DREAM_TEXT_UTF8);
            dream_platform_current->write(2, location, strlen(location), DREAM_TEXT_UTF8);
            dream_platform_current->write(2, "\n", 1, DREAM_TEXT_UTF8);
        }
    }
    /* abort does not flush, and a panic must not hide the output that led to it.
     * Wasm has no hosted stdio buffer; its writes already go straight to the host. */
#ifndef __wasm__
    fflush(stdout);
    fflush(stderr);
#endif
    dream_platform_abort();
}

void dream_panic(dream_ptr msg) { dream_panic_at(msg, NULL); }
