# Iterable and yield helpers

No import is needed.

Read [Yield and lazy sequences](../../language/functions/iterators.md) for sequence creation, cursor access, and cleanup. `Iterable<T>` creates synchronous cursors; `AsyncIterable<T>` creates cursors whose advancement may await. Yield sequences support lazy production and explicit cursor cleanup.

[All sections](core-iterable.md)

## `YieldIterable`

A repeatable synchronous sequence backed by a producer factory.

```dream
public class YieldIterable<T>: Iterable<T>
```

## `constructor`

Creates a `YieldIterable` with `factory`.

```dream
public constructor(factory: fun(YieldChannel<T>): Future<void>)
```

## `iterator`

Creates a cursor over the sequence. Close it when iteration ends early.

```dream
public fun iterator(): Iterator<T>
```

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

## `YieldAsyncIterable`

A repeatable asynchronous sequence backed by a producer factory.

```dream
public class YieldAsyncIterable<T>: AsyncIterable<T>
```

## `constructor`

Creates a `YieldAsyncIterable` with `factory`.

```dream
public constructor(factory: fun(YieldChannel<T>): Future<void>)
```

## `iterator`

Creates a cursor over the sequence. Close it when iteration ends early.

```dream
public fun iterator(): AsyncIterator<T>
```

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

## `YieldAdvanceLease`

Releases a producer's pending advancement when the consumer's scope ends.

```dream
public class YieldAdvanceLease<T>
```

## `constructor`

Creates a `YieldAdvanceLease` with `cursor`.

```dream
public constructor(cursor: YieldAsyncCursor<T>)
```

## `complete`

Completes the pending signal or advancement operation.

```dream
public fun complete(): void
```
