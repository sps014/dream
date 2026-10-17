# TaskPoolError

**Import:** `import system.task;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `TaskPoolError`

Failures scheduling or closing a reusable worker pool.

```dream
public enum TaskPoolError
```

## `InvalidSize`

Pool sizes must be positive.

```dream
InvalidSize
```

## `Closed`

The pool no longer accepts work.

```dream
Closed
```

## `Cancelled`

Cancellation was requested; any submitted work has settled before this is returned.

```dream
Cancelled
```
