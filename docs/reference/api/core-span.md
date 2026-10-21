# Span

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Span`

A writable, bounds-checked view of a contiguous range in an array. Changes affect the original array. Spans keep their source alive, cannot be stored in heap objects, and cannot cross an `await`.

```dream
public ref struct Span<T>
```

## `constructor`

A view of `array[offset .. offset + length)`. Traps unless that range lies inside `array`; every later access relies on it.

```dream
public constructor(borrow array: T[], offset: int, length: int)
```

## `of`

A span over the whole of `array`.

```dream
public static fun of(borrow array: T[]): Span<T>
```

## `length`

Number of elements this span covers.

```dream
public get length(): int
```

## `is_empty`

True when this span covers no elements.

```dream
public fun is_empty(): bool
```

## `as_read_only`

The same elements, viewed read-only.

```dream
public fun as_read_only(): ReadOnlySpan<T>
```

## `this`

The element at `index` (relative to this span, not the backing array). Traps if out of range.

```dream
public fun this[index: int]: T
```

## `this`

Overwrites the element at `index` (relative to this span). Traps if out of range.

```dream
public fun this[index: int] = value: T
```

## `slice`

Returns a view of elements from `start` (inclusive) to `end` (exclusive) that shares this span's storage. Panics unless `0 <= start <= end <= length`.

```dream
public fun slice(start: int, end: int): Span<T>
```

## `slice`

Returns a view from `start` to the end of this span. Panics unless `0 <= start <= length`.

```dream
public fun slice(start: int): Span<T>
```

## `index_of`

Index of the first element equal to `value`, or `None`.

```dream
public fun index_of(borrow value: T): Option<int>
```

## `contains`

True when some element equals `value`.

```dream
public fun contains(borrow value: T): bool
```

## `sequence_equal`

True when `other` has the same length and pairwise-equal elements.

```dream
public fun sequence_equal(other: ReadOnlySpan<T>): bool
```

## `sequence_equal`

Returns whether both spans contain equal elements in the same order.

```dream
public fun sequence_equal(other: Span<T>): bool
```

## `copy_from`

Copies every element of `src` into the start of this span. The destination must be large enough. Overlapping ranges are supported.

```dream
public fun copy_from(src: Span<T>): void
```

## `copy_from`

Copies every element of `src` into the start of this span. The destination must be large enough. Overlapping ranges are supported.

```dream
public fun copy_from(src: Span<T>): void where T: unmanaged
```

## `copy_to`

Copies every element into the start of `dst`. Panics if the destination is shorter. Overlapping ranges are supported.

```dream
public fun copy_to(dst: Span<T>): void
```

## `copy_to`

Copies every element into the start of `dst`. Panics if the destination is shorter. Overlapping ranges are supported.

```dream
public fun copy_to(dst: Span<T>): void where T: unmanaged
```

## `fill`

Fills every element of this span with `value`.

```dream
public fun fill(value: T): void
```

## `to_array`

Copies every element of this span into a fresh, independently-owned array.

```dream
public fun to_array(): T[]
```

## `to_array`

Copies the elements into a new array.

```dream
public fun to_array(): T[]where T: unmanaged
```
