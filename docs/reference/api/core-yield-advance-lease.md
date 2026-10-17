# YieldAdvanceLease

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
