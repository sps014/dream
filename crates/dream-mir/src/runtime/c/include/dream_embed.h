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
 * with the UTF-8 message; the process still aborts when the hook returns.
 */
#ifndef DREAM_EMBED_H
#define DREAM_EMBED_H

#include "dream_platform.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Registers the calling thread with the Dream runtime. Idempotent. */
void dream_thread_attach(void);

/* Runs the thread's pending Dream work and unregisters it. Every `NativeCallback` created on the
 * thread must already be released. A no-op on a thread that never attached. */
void dream_thread_detach(void);

/* Takes and gives back one count on a Dream object reference. Both accept NULL. */
void dream_retain(void *ref);
void dream_release(void *ref);

/* `message` is the panic text in UTF-8. `location` is the Dream source location ("file:line") of
 * the panicking statement, or NULL for a panic the runtime raises itself (such as out of memory).
 * Both stay valid only for the call. */
typedef void (*dream_panic_hook)(const char *message, const char *location);

/* Installs `hook` (NULL restores the default). A panic raised while the hook runs skips it. */
void dream_set_panic_hook(dream_panic_hook hook);

/* Configure once before attaching threads or calling exports. The table and its allocation
 * domain must remain valid for the lifetime of the runtime; changing it after use is invalid. */
void dream_set_platform(const dream_platform *platform);

#ifdef __cplusplus
}
#endif

#endif
