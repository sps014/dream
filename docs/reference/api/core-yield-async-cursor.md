# YieldAsyncCursor

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `YieldAsyncCursor`

An asynchronous cursor over values from a yielding producer.

```dream
public class YieldAsyncCursor<T>: AsyncIterator<T>
```

## `constructor`

Creates a `YieldAsyncCursor` with `channel`, `producer`.

```dream
public constructor(channel: YieldChannel<T>, producer: Future<void>)
```

## `next`

Returns the next value, or `None` when the sequence has ended.

```dream
public async fun next(): Option<T>
```

## `close`

Ends iteration and releases resources held by the cursor.

```dream
public fun close(): void
```
