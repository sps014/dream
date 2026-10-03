# Panics

A **panic** is a fatal, non-recoverable runtime error: the program prints a message and halts immediately. There is no `try`/`catch` for panics — Dream has no exception mechanism at all. If you can anticipate a failure and want to handle it, use [`Option<T>`/`Result<T, E>`](../stdlib/option-result.md) instead; reach for a panic only for "this should never happen" conditions.

## What triggers a panic

Dream inserts automatic checks for the operations below. Each prints a message and halts the instant the bad condition is detected:

| Situation | Example |
| --- | --- |
| Array or string index out of range (including negative) | `arr[arr.length]`, `"abc"[-1]` |
| Integer division or remainder by zero | `10 / 0`, `10 % 0` |
| Integer overflow inside a `checked { }` block (see [Primitives](primitives.md#integer-overflow)) | `checked { 2147483647 + 1; }`, `0u - 1u`, `1 << 32` |
| Casting an `object` to the wrong concrete type | `let o: object = "hi"; (int)o;` |
| Reading an `unowned` field after its referent was freed | see [Memory > `weak`/`unowned`](memory.md#advanced-reference-cycles) |

You can also panic explicitly:

```dream
System.panic("unreachable: config was never validated");
```

`System.panic(message: string): void` prints `message` and halts, exactly like an automatic check. Because it returns `void`, it can only be used in statement position — not as part of a larger expression.

## What a panic looks like

A panic prints its message to standard error, then aborts the process — diagnostics stay out of the program's own output, so piping stdout is unaffected:

```
panic: index out of bounds
```

`System.panic(message)` prints exactly the `message` you pass, so include whatever context is useful yourself.

A panic never unwinds: no destructor, `defer`, or caller code runs after it, and it never crosses into C as an exception.

## Embedding a panic hook

C linked into a native program (a `native/` source, or the app embedding Dream) can observe panics with `dream_set_panic_hook` from `dream_embed.h`:

```c
#include <dream_embed.h>

static void on_panic(const char *message, const char *location) {
    log_fatal("dream: %s (%s)", message, location ? location : "unknown location");
}

void install(void) { dream_set_panic_hook(on_panic); }
```

The hook runs on the panicking thread, in place of the default stderr message, with the message in UTF-8. `location` is `NULL` when the panic site recorded no source location, which is currently every site. When the hook returns, the process still aborts. A panic raised inside the hook skips the hook and aborts with the default message.

## Why panics, not undefined behavior

Out-of-bounds indexes, bad casts, and similar checks halt with a message instead of reading or writing arbitrary memory. Bugs fail loudly during development rather than becoming mysterious wrong answers later.
