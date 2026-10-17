# YieldSignals

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
