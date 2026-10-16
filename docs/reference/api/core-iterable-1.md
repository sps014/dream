# Iterable and yield helpers

No import is needed.

Read [Yield and lazy sequences](../../language/functions/iterators.md) for sequence creation, cursor access, and cleanup. `Iterable<T>` creates synchronous cursors; `AsyncIterable<T>` creates cursors whose advancement may await. Yield sequences support lazy production and explicit cursor cleanup.

[All sections](core-iterable.md)

## `Iterable`

A sequence that creates a fresh synchronous cursor for each traversal.

```dream
public interface Iterable<T>
```

## `iterator`

Creates a cursor over the sequence. Close it when iteration ends early.

```dream
borrow fun iterator(): Iterator<T>
```

## `AsyncIterator`

A cursor whose next value may require asynchronous work. Close it when finished.

```dream
public interface AsyncIterator<T>
```

## `next`

Returns the next value, or `None` when the sequence has ended.

```dream
async unique fun next(): Option<T>
```

## `close`

Ends iteration and releases resources held by the cursor.

```dream
unique fun close(): void
```

## `AsyncIterable`

A sequence that creates a fresh asynchronous cursor for each traversal.

```dream
public interface AsyncIterable<T>
```

## `iterator`

Creates a cursor over the sequence. Close it when iteration ends early.

```dream
borrow fun iterator(): AsyncIterator<T>
```

## `YieldSignals`

Signals coordinating a yielding producer and its consumer.

```dream
public static class YieldSignals
```

## `pending`

Creates a signal that stays pending until explicitly completed.

```dream
public static extern fun pending(): Future<void>
```

## `complete`

Completes the pending signal or advancement operation.

```dream
public static extern fun complete(borrow signal: Future<void>): void
```

## `run`

Runs the producer until a value is ready or the producer finishes.

```dream
public static extern fun run(borrow producer: Future<void>, borrow ready: Future<void>): void
```

## `YieldChannel`

A single-value channel coordinating a yielding producer and consumer.

```dream
public class YieldChannel<T>
```

## `value`

Next value waiting for the consumer, or `None` when no value is ready.

```dream
public value: Option<T>
```

## `ended`

Whether the producer has finished yielding values.

```dream
public ended: bool
```

## `ready`

Signal completed when the next yielded value is available.

```dream
public ready: Future<void>
```

## `resume`

Signal allowing the producer to continue after a value is consumed.

```dream
public resume: Future<void>
```

## `constructor`

Creates a `YieldChannel` with default settings.

```dream
public constructor()
```

## `emit`

Publishes a value and waits until the consumer allows the producer to continue.

```dream
public fun emit(value: T): Future<void>
```

## `finish`

Marks the producer complete and wakes a waiting consumer.

```dream
public fun finish(): void
```

## `advance`

Allows the producer to continue after the current value has been consumed.

```dream
public fun advance(): void
```

## `YieldCursor`

A synchronous cursor over values from a yielding producer.

```dream
public class YieldCursor<T>: Iterator<T>
```

## `constructor`

Creates a `YieldCursor` with `channel`, `producer`.

```dream
public constructor(channel: YieldChannel<T>, producer: Future<void>)
```

## `next`

Returns the next value, or `None` when the sequence has ended.

```dream
public fun next(): Option<T>
```

## `close`

Ends iteration and releases resources held by the cursor.

```dream
public fun close(): void
```
