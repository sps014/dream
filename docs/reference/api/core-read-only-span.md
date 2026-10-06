# ReadOnlySpan

No import is needed.

Read the [usage guide](../language/spans.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ref struct ReadOnlySpan<T>`

`ReadOnlySpan<T>` - a `Span<T>` without the writers: no indexer setter, `set`, `fill`, or `copy_from`. Take one when a function borrows a run of elements and promises not to change them. The same `ref struct` escape rules as `Span<T>` apply (see `docs/reference/language/spans.md`).

```dream
public ref struct ReadOnlySpan<T>
```

## `constructor`

A view of `array[offset .. offset + length)`. Traps unless that range lies inside `array`.

```dream
public constructor(borrow array: T[], offset: int, length: int)
```

## `of`

A read-only span over the whole of `array`.

```dream
public static fun of(borrow array: T[]): ReadOnlySpan<T>
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

## `this`

The element at `index` (relative to this span). Traps if out of range.

```dream
public fun this[index: int]: T
```

## `get`

```dream
public fun get(index: int): T
```

## `slice`

A sub-span covering `[start, start + count)`. Traps if the range doesn't fit.

```dream
public fun slice(start: int, count: int): ReadOnlySpan<T>
```

## `slice`

The sub-span from `start` to the end. Traps if `start` is outside `[0, length]`.

```dream
public fun slice(start: int): ReadOnlySpan<T>
```

## `index_of`

Index of the first element equal to `value` (by value equality), or `None`.

```dream
public fun index_of(borrow value: T): Option<int>
```

## `contains`

True when some element equals `value`.

```dream
public fun contains(borrow value: T): bool
```

## `sequence_equal`

True when `other` has the same length and pairwise-equal elements. Element-wise `==` even for unmanaged `T`: a bitwise compare would disagree with `==` on floating-point NaN and -0.0.

```dream
public fun sequence_equal(other: ReadOnlySpan<T>): bool
```

## `copy_to`

Copies every element into `dst` starting at index 0. Traps if `dst` is shorter. Overlap within one array is handled like `Span<T>.copy_from`.

```dream
public fun copy_to(dst: Span<T>): void
```

## `copy_to`

Unmanaged fast path: one `memory.copy`.

```dream
public fun copy_to(dst: Span<T>): void where T : unmanaged
```

## `to_array`

Copies every element into a fresh, independently-owned array.

```dream
public fun to_array(): T[]
```

## `to_array`

```dream
public fun to_array(): T[] where T : unmanaged
```
