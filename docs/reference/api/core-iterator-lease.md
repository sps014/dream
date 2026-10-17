# IteratorLease

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `IteratorLease`

A loop owns a lease even when the caller retains the cursor. Dropping a cancelled coroutine therefore closes the active cursor independently of its other aliases.

```dream
public class IteratorLease<T>
```

## `constructor`

Creates a `IteratorLease` with `cursor`.

```dream
public constructor(cursor: Iterator<T>)
```

## `close`

Ends iteration and releases resources held by the cursor.

```dream
public fun close(): void
```
