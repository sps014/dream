#include "include/dream_rt_native.h"

#include <stdio.h>
#include <stdlib.h>

#define PANIC_MESSAGE_MAX 1024

static dream_panic_hook panic_hook;
/* A hook that itself panics must not recurse into the hook again. */
static _Thread_local int in_panic_hook;

void dream_set_panic_hook(dream_panic_hook hook) {
    __atomic_store_n(&panic_hook, hook, __ATOMIC_RELEASE);
}

/* Panics can fire on allocation failure or with allocator locks held, so the hook's UTF-8 copy
 * lives in a fixed buffer (truncated on a code-point boundary) rather than on the heap. */
static void panic_utf8(dream_ptr msg, char *out, size_t cap) {
    int32_t len = dream_str_len(msg);
    size_t n = 0;
    for (int32_t i = 0; i < len; i++) {
        uint32_t c = dream_char_at_u(msg, i);
        if (c >= 0xD800 && c < 0xDC00 && i + 1 < len) {
            uint32_t lo = dream_char_at_u(msg, i + 1);
            if (lo >= 0xDC00 && lo < 0xE000) {
                c = 0x10000 + ((c - 0xD800) << 10) + (lo - 0xDC00);
                i++;
            }
        }
        if (c >= 0xD800 && c < 0xE000) {
            c = 0xFFFD;
        }
        size_t w = c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
        if (n + w >= cap) {
            break;
        }
        if (w == 1) {
            out[n++] = (char)c;
        } else if (w == 2) {
            out[n++] = (char)(0xC0 | (c >> 6));
            out[n++] = (char)(0x80 | (c & 0x3F));
        } else if (w == 3) {
            out[n++] = (char)(0xE0 | (c >> 12));
            out[n++] = (char)(0x80 | ((c >> 6) & 0x3F));
            out[n++] = (char)(0x80 | (c & 0x3F));
        } else {
            out[n++] = (char)(0xF0 | (c >> 18));
            out[n++] = (char)(0x80 | ((c >> 12) & 0x3F));
            out[n++] = (char)(0x80 | ((c >> 6) & 0x3F));
            out[n++] = (char)(0x80 | (c & 0x3F));
        }
    }
    out[n] = 0;
}

void dream_panic(dream_ptr msg) {
    dream_panic_hook hook = __atomic_load_n(&panic_hook, __ATOMIC_ACQUIRE);
    if (hook && !in_panic_hook) {
        char message[PANIC_MESSAGE_MAX];
        panic_utf8(msg, message, sizeof(message));
        in_panic_hook = 1;
        hook(message, NULL);
    } else {
        print_err_string(msg);
        print_err_char(10);
    }
    abort();
}
