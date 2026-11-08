/* Public embedding API for native Dream executables and libraries. Every native output
 * exports these symbols; the compiler puts this
 * header on the include path of every `native/` source, so `#include <dream_embed.h>` works as-is.
 *
 * Threads. Dream code runs on threads the Dream runtime knows: the main thread and Dream workers.
 * A thread C created itself must call `dream_thread_attach()` before it calls any Dream function
 * pointer (a plain `fun` passed to an `@c` extern) and `dream_thread_detach()` before it exits.
 * `NativeCallback` closures stay bound to the Dream thread that created them; call them from that
 * thread only.
 *
 * References. A Dream object reference handed to C is a pointer the program counts. C that stores
 * one past the call that produced it takes its own count with `dream_retain()` and gives it back
 * with `dream_release()`, on the Dream thread the reference came from: counts are not atomic.
 *
 * Panics. A panic never unwinds into C. By default it prints its message to stderr and aborts the
 * process. A hook installed with `dream_set_panic_hook()` runs first, on the panicking thread,
 * with the UTF-8 message; the process still aborts when the hook returns. The hook sees at most
 * 1024 bytes of that message, including the terminating NUL.
 */
#ifndef DREAM_EMBED_H
#define DREAM_EMBED_H

#include <stddef.h>
#include <stdint.h>

#include "dream_platform.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The layout version of this header. Compare it with `dream_embed_abi_version()` from the
 * binary before calling anything else. */
uint32_t dream_embed_abi_version(void);

/* Registers the calling thread with the Dream runtime. Repeated calls nest; each one needs
 * its own `dream_thread_detach`. A detach never tears down a Dream worker's own registration. */
void dream_thread_attach(void);

/* Drops one nested registration. The matching call for a thread's last registration runs its
 * pending Dream work and unregisters it. Every `NativeCallback` created on the thread must
 * already be released. A no-op on a thread that never attached, and on a Dream worker thread. */
void dream_thread_detach(void);

/* Takes and gives back one count on a Dream object reference. Both accept NULL.
 * Debug and sanitizer builds reject a call from a thread that is neither the runtime's
 * first thread nor a thread that has called `dream_thread_attach`. */
void dream_retain(void *ref);
void dream_release(void *ref);

/* Gives back one count on a `NativeCallback` from any thread. The Dream thread that created
 * the callback applies the release. */
void dream_callback_release(void *callback);

/* Completes the future an `@async_host` extern handed to its `<name>Async` host, from any thread.
 * `result` carries the value as the host's synchronous return would (zero for `void`). The
 * future stays valid until this call; a second completion, or one after the Dream side cancelled
 * the future, is ignored. */
void dream_complete_foreign(void *future, uint64_t result);

/* `message` is the panic text in UTF-8. `location` is the Dream source location ("file:line") of
 * the panicking statement, or NULL for a panic the runtime raises itself (such as out of memory).
 * Both stay valid only for the call. */
typedef void (*dream_panic_hook)(const char *message, const char *location);

/* Installs `hook` (NULL restores the default). A panic raised while the hook runs skips it. */
void dream_set_panic_hook(dream_panic_hook hook);

/* Configure once before attaching threads or calling exports. The table and its allocation
 * domain must remain valid for the lifetime of the runtime; changing it after use is invalid. */
void dream_set_platform(const dream_platform *platform);

/* Runs one pass of this thread's ready Dream work and due timers. Work that becomes ready
 * during the pass runs on the next call, so a frame loop yields once per call. Returns 0 when
 * nothing is outstanding, 1 when runnable work is queued (the caller paces a visible frame),
 * and on Android 2 after waiting in the host event park because only a timer or foreign
 * completion was outstanding. A call made while a pump is already running returns 1 without
 * running anything. Call it from the platform event loop on the thread that attached the runtime. */
int dream_scheduler_pump(void);

/* A `byte[]` of `len` zero bytes. The pointer is the Dream array. `dream_array_data` is its
 * first byte. Dream frees the array when the `@c` result is dropped. */
void *dream_bytes_new(int32_t len);

/* An array of `len` elements, each `element_size` bytes, zeroed. Return it from an `@c`
 * function whose result is that array type. */
void *dream_scalar_array_new(int32_t element_size, int32_t len);

/* The first byte of a Dream array, or NULL when `array` is NULL. */
void *dream_array_data(void *array);

/* A Dream string copied from UTF-8 or UTF-16. Return it from an `@owned` `@c` function.
 * `len` is the number of bytes or UTF-16 units, not a NUL-terminated length. */
void *dream_string_from_utf8(const char *bytes, size_t len);
void *dream_string_from_utf16(const uint16_t *units, size_t len);

/* The UTF-16 units and unit count of a Dream string. A `borrow string` parameter arrives as
 * `dream_str_view` instead; that view is valid only for the call. */
const uint16_t *dream_string_units(void *s);
int32_t dream_string_length(void *s);

typedef struct dream_str_view {
    const uint16_t *units;
    uintptr_t len;
} dream_str_view;

/* Finish an async `@c` call. The C function receives the future as its first argument and
 * returns 0 when it has already completed, or any other value when it will complete later
 * from any thread. A second completion is ignored. `dream_future_complete_ref` takes ownership
 * of a Dream string or array. `dream_future_complete_error` makes the awaiting Dream code panic
 * with `code` and `message`. */
void dream_future_complete_u64(void *future, uint64_t value);
void dream_future_complete_ref(void *future, void *ref);
void dream_future_complete_error(void *future, int32_t code, const char *message);

/* Runs `fn(ud)` on the thread that cancels the future. The future is already cancelled. */
void dream_future_on_cancel(void *future, void (*fn)(void *), void *ud);

/* The Dream thread this call is running on, or NULL when the thread is not attached. */
void *dream_current_owner(void);

/* Runs `fn(ud)` on `owner`'s Dream thread. A call on that thread runs `fn` immediately.
 * Pass NULL to use the calling thread's owner. */
void dream_post(void *owner, void (*fn)(void *), void *ud);

#ifdef __cplusplus
}
#endif

#endif
