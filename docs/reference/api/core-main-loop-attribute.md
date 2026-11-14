# MainLoopAttribute

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `MainLoopAttribute`

Hands the main thread to a library's event loop on Android and iOS, where the operating system owns that thread and a program cannot block in its own loop.

Put it on an `extern` function declared as `(argc: int, argv: isize, entry: isize, pump: isize): int`. On those targets the program's generated `main` calls this function instead of running `main` directly. The function must call `entry` (a C `int (*)(void)`) once to start the program, then call `pump` (the same shape) on every iteration of its loop to run ready `async` work, and return the process exit code. `argv` is the platform's `char**`.

A program may link at most one main-loop function. Other targets ignore the attribute, and programs that link none run `main` directly.

```dream
public struct MainLoopAttribute
```
