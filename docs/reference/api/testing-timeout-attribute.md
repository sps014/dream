# TimeoutAttribute

**Import:** `import system.testing;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `TimeoutAttribute`

Fails a `@test` function that runs longer than this many milliseconds. Tests have no timeout unless they declare one.

```dream
public struct TimeoutAttribute
```

## `milliseconds`

Timeout or interval in milliseconds.

```dream
public milliseconds: int
```
