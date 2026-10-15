# Iterable and yield helpers

No import is needed.

Read [Yield and lazy sequences](../language/iterators.md) for sequence creation, cursor access, and cleanup. `Iterable<T>` creates synchronous cursors; `AsyncIterable<T>` creates cursors whose advancement may await. The yield channel, cursor, signal, and lease classes support compiler-generated yield functions.

[All sections](core-iterable.md)

## `interface Iterable<T>`

```dream
public interface Iterable<T>
```

## `borrow fun iterator(): Iterator<T>`

```dream
borrow fun iterator(): Iterator<T>
```

## `interface AsyncIterator<T>`

```dream
public interface AsyncIterator<T>
```

## `async unique fun next(): Option<T>`

```dream
async unique fun next(): Option<T>
```

## `unique fun close(): void`

```dream
unique fun close(): void
```

## `interface AsyncIterable<T>`

```dream
public interface AsyncIterable<T>
```

## `borrow fun iterator(): AsyncIterator<T>`

```dream
borrow fun iterator(): AsyncIterator<T>
```

## `static class YieldSignals`

```dream
public static class YieldSignals
```

## `static extern fun pending(): Future<void>`

```dream
public static extern fun pending(): Future<void>
```

## `static extern fun complete(borrow signal: Future<void>): void`

```dream
public static extern fun complete(borrow signal: Future<void>): void
```

## `static extern fun run(borrow producer: Future<void>, borrow ready: Future<void>): void`

```dream
public static extern fun run(borrow producer: Future<void>, borrow ready: Future<void>): void
```

## `class YieldChannel<T>`

```dream
public class YieldChannel<T>
```

## `value: Option<T>`

```dream
public value: Option<T>
```

## `ended: bool`

```dream
public ended: bool
```

## `ready: Future<void>`

```dream
public ready: Future<void>
```

## `resume: Future<void>`

```dream
public resume: Future<void>
```

## `constructor()`

```dream
public constructor()
```

## `fun emit(value: T): Future<void>`

```dream
public fun emit(value: T): Future<void>
```

## `fun finish(): void`

```dream
public fun finish(): void
```

## `fun advance(): void`

```dream
public fun advance(): void
```

## `class YieldCursor<T> : Iterator<T>`

```dream
public class YieldCursor<T> : Iterator<T>
```

## `constructor(channel: YieldChannel<T>, producer: Future<void>)`

```dream
public constructor(channel: YieldChannel<T>, producer: Future<void>)
```

## `fun next(): Option<T>`

```dream
public fun next(): Option<T>
```

## `fun close(): void`

```dream
public fun close(): void
```
