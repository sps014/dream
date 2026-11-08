---
title: Panics
summary: "Stop the program on unrecoverable errors, and when to use `Result` instead."
icon: material/alert-octagon-outline
status: stable
targets: [native, web, node]
---

# Panics

A panic stops the program immediately and prints an error. Use returned `Result` values for failures your program should handle. Panics are for situations where continuing would be wrong.

!!! glance "At a glance"

    ```dream
    System.panic("unreachable: config was never validated");
    ```

    - A panic stops the program immediately with an error message.
    - Return `Result` for failures your program should handle.

## What triggers a panic

Dream inserts automatic checks for the operations below. Each prints a message and halts the instant the bad condition is detected:

| Situation | Example |
| --- | --- |
| Array or string index out of range (including negative) | `arr[arr.length]`, `"abc"[-1]` |
| Integer division or remainder by zero | `10 / 0`, `10 % 0` |
| Integer overflow inside a `checked { }` block (see [Primitives](../types/primitives.md#integer-overflow)) | `checked { 2147483647 + 1; }`, `0u - 1u`, `1 << 32` |
| Casting an `object` to the wrong concrete type | `let o: object = "hi"; (int)o;` |
| Reading an `unowned` field after its referent was freed | see [Memory > `weak`/`unowned`](../memory/cycles.md#weak-and-unowned-references) |

You can also panic explicitly:

```dream
System.panic("unreachable: config was never validated");
```

`System.panic(message: string): void` prints `message` and halts, exactly like an automatic check. Because it returns `void`, it can only be used in statement position — not as part of a larger expression.

## What a panic looks like

A panic prints its message and a source location to standard error, then stops the process. The program's own output is unchanged:

```
panic: index out of bounds
  at /home/me/app/src/main.dream:12
```

`System.panic(message)` prints exactly the `message` you pass, followed by the same location line.

- The location is the file and line of the statement.
- A panic inside the standard library or a package points at your call: `xs[10]`, `Option.unwrap()` on `None`, and a package bug in a direct call.
- A library function reached through a function value or an interface names its own line. Your lambdas keep their own lines.
- Running out of memory has no location.

Nothing runs after a panic. `del`, `defer`, and the rest of the caller are skipped. It is not a C exception.

## Embedding a panic hook

C linked into a native program (a `native/` source, or the app embedding Dream) can observe panics with `dream_set_panic_hook` from `dream_embed.h`:

```c
#include <dream_embed.h>

static void on_panic(const char *message, const char *location) {
    log_fatal("dream: %s (%s)", message, location ? location : "unknown location");
}

void install(void) { dream_set_panic_hook(on_panic); }
```

The hook runs on the panicking thread instead of the default message. `message` is UTF-8. `location` is `file:line`, or `NULL` when there is no source line. The process still stops after the hook returns. A panic inside the hook uses the default message.

## Why panics, not undefined behavior

An out-of-bounds index or a bad cast stops the program with a message.
