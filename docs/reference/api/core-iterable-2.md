# Iterable and yield helpers

No import is needed.

Read [Yield and lazy sequences](../language/iterators.md) for sequence creation, cursor access, and cleanup. `Iterable<T>` creates synchronous cursors; `AsyncIterable<T>` creates cursors whose advancement may await. The yield channel, cursor, signal, and lease classes support compiler-generated yield functions.

[All sections](core-iterable.md)

## `class YieldIterable<T> : Iterable<T>`

```dream
public class YieldIterable<T> : Iterable<T>
```

## `constructor(factory: fun(YieldChannel<T>): Future<void>)`

```dream
public constructor(factory: fun(YieldChannel<T>): Future<void>)
```

## `fun iterator(): Iterator<T>`

```dream
public fun iterator(): Iterator<T>
```

## `class YieldAsyncCursor<T> : AsyncIterator<T>`

```dream
public class YieldAsyncCursor<T> : AsyncIterator<T>
```

## `constructor(channel: YieldChannel<T>, producer: Future<void>)`

```dream
public constructor(channel: YieldChannel<T>, producer: Future<void>)
```

## `async fun next(): Option<T>`

```dream
public async fun next(): Option<T>
```

## `fun close(): void`

```dream
public fun close(): void
```

## `class YieldAsyncIterable<T> : AsyncIterable<T>`

```dream
public class YieldAsyncIterable<T> : AsyncIterable<T>
```

## `constructor(factory: fun(YieldChannel<T>): Future<void>)`

```dream
public constructor(factory: fun(YieldChannel<T>): Future<void>)
```

## `fun iterator(): AsyncIterator<T>`

```dream
public fun iterator(): AsyncIterator<T>
```

## `class IteratorLease<T>`

A loop owns a lease even when the caller retains the cursor. Dropping a cancelled coroutine therefore closes the active cursor independently of its other aliases.

```dream
public class IteratorLease<T>
```

## `constructor(cursor: Iterator<T>)`

```dream
public constructor(cursor: Iterator<T>)
```

## `fun close(): void`

```dream
public fun close(): void
```

## `class AsyncIteratorLease<T>`

```dream
public class AsyncIteratorLease<T>
```

## `constructor(cursor: AsyncIterator<T>)`

```dream
public constructor(cursor: AsyncIterator<T>)
```

## `fun close(): void`

```dream
public fun close(): void
```

## `class YieldAdvanceLease<T>`

```dream
public class YieldAdvanceLease<T>
```

## `constructor(cursor: YieldAsyncCursor<T>)`

```dream
public constructor(cursor: YieldAsyncCursor<T>)
```

## `fun complete(): void`

```dream
public fun complete(): void
```

