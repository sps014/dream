# Range

**Import:** `import system.core;`

Ranges are also available without an explicit import. See [Ranges](../../language/types/ranges.md) for runnable examples, including descending traversal.

## `Range`

An exclusive integer range that counts toward its end, ascending or descending.

```dream
public struct Range
```

## `start: int`

The first value, included unless the range is empty.

```dream
public start: int
```

## `end: int`

The stopping value, always excluded.

```dream
public end: int
```

## `constructor`

Creates a range from `start` toward the excluded `end`, with unit steps.

```dream
public constructor(start: int, end: int)
```

## `length`

The number of elements. Panics if the distance exceeds the maximum int (2147483647).

```dream
public get length(): int
```

## `is_empty`

True when the start and end are equal.

```dream
public get is_empty(): bool
```

## `contains`

True when `value` occurs before the excluded end in either direction.

```dream
public fun contains(value: int): bool
```

## `iterator`

Creates a fresh cursor that visits the range in unit steps toward its end.

```dream
public fun iterator(): RangeIterator
```
