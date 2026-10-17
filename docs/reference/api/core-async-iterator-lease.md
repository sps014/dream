# AsyncIteratorLease

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `AsyncIteratorLease`

Closes an asynchronous cursor when its iteration scope ends.

```dream
public class AsyncIteratorLease<T>
```

## `constructor`

Creates a `AsyncIteratorLease` with `cursor`.

```dream
public constructor(cursor: AsyncIterator<T>)
```

## `close`

Ends iteration and releases resources held by the cursor.

```dream
public fun close(): void
```
