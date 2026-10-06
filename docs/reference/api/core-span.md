# Span

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ref struct Span<T>`

A Span views part of an existing array without copying its elements. Its positions are checked against that view's range. It keeps the array alive while in use. As a `ref struct`, it cannot be stored or passed in ways that let it outlive the current function. Read [array views](../language/arrays.md#spant-a-bounds-checked-view-without-copying) for examples and restrictions.

```dream
public ref struct Span<T>
```

## `constructor`

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

The element at `index` (relative to this span, not the backing array). Stops the program if out of range.

```dream
public fun this[index: int]: T
```

## `get`

```dream
public fun get(index: int): T
```

## `this`

Overwrites the element at `index` (relative to this span). Stops the program if out of range.

```dream
public fun this[index: int] = value: T
```

## `set`

```dream
public fun set(index: int, value: T): void
```

## `slice`

A sub-span covering `[start, start + count)` of this span's own range. Stops the program if the requested range doesn't fit within this span.

```dream
public fun slice(start: int, count: int): Span<T>
```

## `slice`

The sub-span from `start` to the end. Traps if `start` is outside `[0, length]`.

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

```dream
public fun sequence_equal(other: Span<T>): bool
```

## `copy_from`

Copies every element of `src` into this span starting at index 0. Stops the program if `src` is longer than this span. Reference elements go through ordinary assignment (retain/release); the unmanaged specialization below bulk-blits instead.

```dream
public fun copy_from(src: Span<T>): void
```

## `copy_from`

Unmanaged fast path: one `memory.copy` of `src._length * sizeof(T)` bytes.

```dream
public fun copy_from(src: Span<T>): void where T : unmanaged
```

## `copy_to`

Copies every element of this span into `dst` starting at index 0. Traps if `dst` is shorter.

```dream
public fun copy_to(dst: Span<T>): void
```

## `copy_to`

```dream
public fun copy_to(dst: Span<T>): void where T : unmanaged
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

Unmanaged fast path: allocate then bulk-blit.

```dream
public fun to_array(): T[] where T : unmanaged
```
