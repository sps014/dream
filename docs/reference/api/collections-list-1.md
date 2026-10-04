# List

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections/list.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](collections-list.md)

## `class List<T> : IndexedCollection<T>`

Growable random-access sequence backed by a resizable buffer. Invariant: `0 <= count <= items.length`. Every mutation of `items`/`count` preserves it, which is what makes the `Buffer.get_unchecked`/`set_unchecked` accesses on `[0, count)` sound.

```dream
public class List<T> : IndexedCollection<T>
```

## `constructor`

Allocates an empty list with an initial backing buffer capacity (defaults to 8).

```dream
public constructor(capacity: int = 8)
```

## `with_capacity`

Allocates an empty list with backing capacity `capacity`.

```dream
public static fun with_capacity(capacity: int): List<T>
```

## `length`

Number of elements currently stored.

```dream
public get length(): int
```

## `is_empty`

True when the list has no elements.

```dream
public fun is_empty(): bool
```

## `capacity`

Current capacity of the backing buffer.

```dream
public get capacity(): int
```

## `push`

Appends a value to the end, growing if necessary.

```dream
public fun push(value: T): void
```

## `push_all`

Appends every element of `items`, ensuring capacity once then copying via `Span`.

```dream
public fun push_all(borrow items: T[]): void
```

## `from_array`

Builds a list containing exactly the elements of `items`, in order. The `[e1, e2, ...]` list-literal syntax (e.g.

```dream
public static fun from_array(borrow items: T[]): List<T>
```

## `take_array`

Yields a `T[]` of exactly `length` and leaves this list empty. Shrinks via `realloc` when capacity is larger than `count` so the result's `.length` is the live element count.

```dream
public fun take_array(): T[]
```

## `this`

Element at `index`. Panics if `index` is out of range.

```dream
public fun this[index: int]: T
```

## `this`

Overwrites the element at `index`. Panics if `index` is out of range.

```dream
public fun this[index: int] = value: T
```

## `set`

```dream
public fun set(index: int, value: T): void
```

## `pop`

Removes and returns the last element, or `None` if the list is empty.

```dream
public fun pop(): Option<T>
```

## `contains`

True if `value` is present (uses value equality, including string contents).

```dream
public fun contains(borrow value: T): bool
```

## `index_of`

Index of the first matching element (by value equality), or `None` if absent.

```dream
public fun index_of(borrow value: T): Option<int>
```

## `last_index_of`

Index of the last matching element (by value equality), or `None` if absent.

```dream
public fun last_index_of(borrow value: T): Option<int>
```

## `reserve`

Grows the backing buffer until `capacity()` is at least `n`.

```dream
public fun reserve(n: int): void
```

## `reverse`

Reverses the live prefix in place.

```dream
public fun reverse(): void
```

## `slice`

A new list of `this[start .. end)` (clamped; empty when the span is inverted).

```dream
public fun slice(start: int, end: int): List<T>
```

## `concat`

A new list with this list's elements followed by `other`.

```dream
public fun concat(borrow other: List<T>): List<T>
```

## `clone`

A shallow copy of the live elements.

```dream
public fun clone(): List<T>
```

## `join`

Joins stringifiable elements with `sep` (same conversion as `to_string` uses). Single pass into a StringBuilder — O(total output), never the quadratic accumulate-and-recopy shape.

```dream
public fun join(sep: string): string
```

## `clear`

Empties the list while keeping the backing buffer. Live slots are overwritten with a zero value so reference fields inside value-struct elements (and class elements) release immediately; capacity is unchanged (no capacity-sized `Buffer.alloc`).

```dream
public fun clear(): void
```
