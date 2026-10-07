# Panics

A panic stops the program immediately and prints an error. Use returned `Result` values for failures your program should handle. Panics are for situations where continuing would be wrong.

## What triggers a panic

Dream inserts automatic checks for the operations below. Each prints a message and halts the instant the bad condition is detected:

| Situation | Example |
| --- | --- |
| Array or string index out of range (including negative) | `arr[arr.length]`, `"abc"[-1]` |
| Integer division or remainder by zero | `10 / 0`, `10 % 0` |
| Integer overflow inside a `checked { }` block (see [Primitives](primitives.md#integer-overflow)) | `checked { 2147483647 + 1; }`, `0u - 1u`, `1 << 32` |
| Casting an `object` to the wrong concrete type | `let o: object = "hi"; (int)o;` |
| Reading an `unowned` field after its referent was freed | see [Memory > `weak`/`unowned`](memory-cycles.md#weak-and-unowned-references) |

You can also panic explicitly:

```dream
System.panic("unreachable: config was never validated");
```

`System.panic(message: string): void` prints `message` and halts, exactly like an automatic check. Because it returns `void`, it can only be used in statement position — not as part of a larger expression.

## What a panic looks like

A panic prints its message and the source location of the statement that panicked to standard error, then aborts the process — diagnostics stay out of the program's own output, so piping stdout is unaffected:

```
panic: index out of bounds
  at /home/me/app/src/main.dream:12
```

`System.panic(message)` prints exactly the `message` you pass, followed by the same location line.

The location names the file as the compiler saw it and the line of the statement. A panic inside library code — the standard library or a package in `dream_packages/` — names the line in your program that called into the library, not the library's own source: `xs[10]` on a `List`, `Option.unwrap()` on `None`, or a package function that misuses a `List` all point at your call. A library function reached only through a function value or an interface call names its own line instead. Your own lambdas keep their own lines, even when a library function calls them. Panics raised by the runtime itself, such as running out of memory, have no location.

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

The hook runs on the panicking thread, in place of the default stderr message, with the message in UTF-8 and `location` as the same `file:line` the default message prints. `location` is `NULL` for panics raised by the runtime itself, which have no source line. When the hook returns, the process still aborts. A panic raised inside the hook skips the hook and aborts with the default message.

## Why panics, not undefined behavior

Out-of-bounds indexes, bad casts, and similar checks halt with a message instead of reading or writing arbitrary memory. Bugs fail loudly during development rather than becoming mysterious wrong answers later.
