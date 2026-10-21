# List

**Import:** `import system.collections;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `List`

A growable, ordered sequence with zero-based indexed access. Adding elements may increase its capacity.

```dream
public class List<T>: IndexedCollection<T>
```

## `constructor`

Allocates an empty list with an initial backing buffer capacity (defaults to 8).

```dream
public constructor(capacity: int = 8)
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

## `reserve`

Reserves room for additional elements without changing length or existing values.

```dream
public fun reserve(additional: int): Result<Unit, CapacityError>
```

## `push`

Appends a value to the end, growing if necessary.

```dream
public fun push(value: T): void
```

## `push_all`

Appends every element of `items` in order.

```dream
public fun push_all(borrow items: T[]): void
```

## `constructor`

Creates a list containing the elements of `items` in order. List literals such as `[1, 2, 3]` create lists in the same way.

```dream
public constructor(borrow items: T[])
```

## `take_array`

Returns the elements as an array and leaves the list empty. The returned array has exactly the previous list length.

```dream
public fun take_array(): T[]
```

## `this`

Returns the element at `index`. Panics if `index` is out of range.

```dream
public fun this[index: int]: T
```

## `this`

Overwrites the element at `index`. Panics if `index` is out of range.

```dream
public fun this[index: int] = value: T
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

## `reverse`

Reverses the live prefix in place.

```dream
public fun reverse(): void
```

## `slice`

Returns a new list holding the elements from `start` (inclusive) to `end` (exclusive). Panics unless `0 <= start <= end <= length`.

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

Removes all elements and releases the references they held. Capacity is unchanged.

```dream
public fun clear(): void
```

## `clear`

Removes all elements and releases the references they held. Capacity is unchanged.

```dream
public fun clear(): void where T: unmanaged
```

## `insert`

Inserts `value` at `index`, shifting later elements right. Returns false if the index is outside 0 through length.

```dream
public fun insert(index: int, value: T): bool
```
