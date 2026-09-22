# Panics

A **panic** is a fatal, non-recoverable runtime error: the program prints a message and halts immediately. There is no `try`/`catch` for panics — Dream has no exception mechanism at all. If you can anticipate a failure and want to handle it, use [`Option<T>`/`Result<T, E>`](../stdlib/option-result.md) instead; reach for a panic only for "this should never happen" conditions.

## What triggers a panic

Dream inserts automatic checks for the operations below. Each prints a message and halts the instant the bad condition is detected:

| Situation | Example |
| --- | --- |
| Array or string index out of range (including negative) | `arr[arr.length]`, `"abc"[-1]` |
| Integer division or remainder by zero | `10 / 0`, `10 % 0` |
| Casting an `object` to the wrong concrete type | `let o: object = "hi"; (int)o;` |
| Reading an `unowned` field after its referent was freed | see [Memory > `weak`/`unowned`](memory.md#advanced-reference-cycles) |

You can also panic explicitly:

```dream
System.panic("unreachable: config was never validated");
```

`System.panic(message: string): void` prints `message` and halts, exactly like an automatic check. Because it returns `void`, it can only be used in statement position — not as part of a larger expression.

## What a panic looks like

A panic prints its message to standard error, then halts the program — diagnostics stay out of the program's own output, so piping stdout is unaffected. The automatic checks' messages include the failing source file, line, and declaring function, e.g.:

```
panic: index out of bounds (at /path/to/program.dream:6, in main)
```

`System.panic(message)` prints exactly the `message` you pass — no automatic location is appended, so include whatever context is useful yourself.

!!! note "Precision notes"
    The line is the checked construct's own source line whenever Dream can determine it (`?` otherwise). A check inside a small function that was copied into the caller may show the caller's line — still diagnosable, not a wrong file.

## Why panics, not undefined behavior

Out-of-bounds indexes, bad casts, and similar checks halt with a message instead of reading or writing arbitrary memory. Bugs fail loudly during development rather than becoming mysterious wrong answers later.
