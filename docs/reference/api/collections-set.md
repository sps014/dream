# Set

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections/set.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Set<T> : Collection<T>`

A generic open-addressing hash set.

```dream
public class Set<T> : Collection<T>
```

## `constructor`

Allocates an empty set with an initial capacity (defaults to 8, rounded up to power of 2).

```dream
public constructor(capacity: int = 8)
```

## `length`

Number of elements currently stored.

```dream
public get length(): int
```

## `capacity`

Current capacity of the backing buffer.

```dream
public get capacity(): int
```

## `add`

Inserts a value into the set. Returns true if it was newly added, false if already present.

```dream
public fun add(value: T): bool
```

## `contains`

True if the set contains the value.

```dream
public fun contains(borrow value: T): bool
```

## `contains`

True when the set holds the `string` equal to `value`, without building a `string`.

```dream
public fun contains(value: ReadOnlySpan<char>): bool where T : StringKey
```

## `add_all`

Adds every element of `items` (duplicates are ignored, exactly like repeated `add` calls). Also what the `{e1, e2, ...}` set-literal syntax lowers to via `from_array`, so a literal with N elements costs one call, not N.

```dream
public fun add_all(borrow items: T[]): void
```

## `from_array`

Builds a set containing exactly the (deduplicated) elements of `items`. The `{e1, e2, ...}` set-literal syntax (e.g.

```dream
public static fun from_array(borrow items: T[]): Set<T>
```

## `remove`

Removes a value and returns true if it was removed.

```dream
public fun remove(borrow value: T): bool
```

## `is_empty`

True if the set is empty.

```dream
public fun is_empty(): bool
```

## `clear`

Removes all elements, keeping the current capacity. Occupied key slots are zeroed so managed entries release; probe `states` are reset in place (no capacity realloc).

```dream
public fun clear(): void
```

## `clear`

```dream
public fun clear(): void where T : unmanaged
```

## `to_array`

Gets all elements as an array.

```dream
public fun to_array(): T[]
```

## `union`

Elements present in this set or `other`.

```dream
public fun union(borrow other: Set<T>): Set<T>
```

## `intersection`

Elements present in both sets.

```dream
public fun intersection(borrow other: Set<T>): Set<T>
```

## `difference`

Elements in this set but not in `other`.

```dream
public fun difference(borrow other: Set<T>): Set<T>
```

## `is_subset`

True when every element of this set is in `other`.

```dream
public fun is_subset(borrow other: Set<T>): bool
```

## `is_disjoint`

True when the sets share no elements.

```dream
public fun is_disjoint(borrow other: Set<T>): bool
```

## `iterator`

Gets an iterator for the set.

```dream
public fun iterator(): SetIterator<T>
```

## `to_string`

```dream
public override fun to_string(): string
```
