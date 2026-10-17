# YieldIterable

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
