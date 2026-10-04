# Callbacks from native libraries

Let a native library call a Dream function. Match the expected arguments and keep the callback alive for as long as the library needs it.

[Back to overview](c-interop.md)

## Callbacks

There are two ways to hand C a function.

**A plain `fun`** becomes a C function pointer when it is statically known: a named function or a
non-capturing lambda written at the call. When the C signature needs conversion (a `const char*`
parameter arriving as `string`, or a pointer arriving as `CPtr`), the compiler emits a small C-ABI
wrapper for that function. Passing a `fun` held in a variable where conversion is needed is an
error that points at `NativeCallback`.

```dream
@c("c") extern fun qsort(base: int[], n: usize, size: usize, cmp: fun(CPtr, CPtr): int): void;

qsort(xs, (usize)5, (usize)sizeof(int), (a: CPtr, b: CPtr) => Ffi.read_int(a, 0) - Ffi.read_int(b, 0));
```

**`NativeCallback<F>`** (in `system`) wraps any `fun`, capturing or not. Use it for C APIs that
take a function pointer plus a `void* user_data`. The parameter expands to both C arguments. By
default the callback receives `user_data` as its first C parameter. Mark the parameter
`@marshal("user_data_last")` for APIs that pass it last.

```c
typedef int (*ticker_fn)(void* user, int tick);
void ticker_on_tick(ticker* t, ticker_fn fn, void* user);
```

```dream
@c extern fun ticker_on_tick(t: CPtr, cb: NativeCallback<fun(int): int>): void;

public class Ticker {
    handle: CPtr;
    listener: Option<NativeCallback<fun(int): int>>;

    public fun on_tick(f: fun(int): int): void {
        let cb = NativeCallback<fun(int): int>(f);
        ticker_on_tick(this.handle, cb);
        this.listener = Option.Some(cb); // C keeps calling it: keep it alive
    }
}
```

The callback's lifetime is ordinary ARC. Keep the `NativeCallback` in a field for as long as C may
call it. Pass a temporary when C only calls it during the call (`qsort_r`, `sqlite3_exec`).

Each `NativeCallback` belongs to the Dream thread that constructed it. Calls and retains must
run on that owner, even when another thread is a Dream `Task` worker; other threads trap with an
owner-thread diagnostic.

A plain `fun` passed as a C function pointer has no owner. A thread C started itself may call it
after `dream_thread_attach()`, and must call `dream_thread_detach()` before it exits. Calling
without attaching traps with a message that names `dream_thread_attach`. Both functions are
declared in `dream_embed.h`, which every `native/` source can include:

```c
#include <dream_embed.h>

static void* worker(void* arg) {
    struct job* j = arg;
    dream_thread_attach();
    j->result = j->fn(j->input); // a Dream `fun` passed to C
    dream_thread_detach();
    return 0;
}
```

`dream_embed.h` also declares `dream_retain`/`dream_release` (for C that stores a Dream reference
past a call) and `dream_set_panic_hook` (see [Panics](panics.md#embedding-a-panic-hook)).

C++ adapters may release their retained callback from any thread. Such releases are queued
without touching the object's non-atomic reference count; the owner drains them at scheduler
ticks, worker wakeups, or shutdown. Destructors run on the owner. Join foreign users and release
their callback handles before the owner exits; a callback that outlives its owner is an error.
