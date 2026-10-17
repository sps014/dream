# YieldChannel

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
