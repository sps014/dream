# SortedMap

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections/sorted-map.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class SortedMap<K : Comparable<K>, V>`

An ordered map backed by parallel sorted arrays (binary search + shift on insert/remove). O(log n) lookup, O(n) insert/remove, ascending-key iteration. Prefer `Map<K, V>` when key order doesn't matter — it has O(1) average insert/lookup.

```dream
public class SortedMap<K : Comparable<K>, V>
```

## `constructor`

Allocates an empty sorted map with an initial capacity (defaults to 8).

```dream
public constructor(capacity: int = 8)
```

## `length`

Number of key/value pairs currently stored.

```dream
public get length(): int
```

## `is_empty`

True when the map is empty.

```dream
public fun is_empty(): bool
```

## `clear`

Removes every entry, keeping capacity. Live slots are overwritten with a zero value so reference keys/values release immediately.

```dream
public fun clear(): void
```

## `this`

Inserts or updates the value for a key. O(log n) to locate, O(n) worst case to shift.

```dream
public fun this[key: K] = value: V
```

## `set`

```dream
public fun set(key: K, value: V): void
```

## `get`

Value for `key`, or `None` when the key is absent.

```dream
public fun get(borrow key: K): Option<V>
```

## `this`

Value for `key`. Panics when the key is absent — this is what `sm[k]` compiles to; use `get(k)` for absence-tolerant reads.

```dream
public fun this[borrow key: K]: V
```

## `contains`

True if the map contains the key.

```dream
public fun contains(borrow key: K): bool
```

## `remove`

Removes a key and returns true if it was present.

```dream
public fun remove(borrow key: K): bool
```

## `first_key`

The smallest key, or `None` when empty.

```dream
public fun first_key(): Option<K>
```

## `last_key`

The largest key, or `None` when empty.

```dream
public fun last_key(): Option<K>
```

## `ceiling_key`

The smallest key `>= key`, or `None` if every key is smaller.

```dream
public fun ceiling_key(borrow key: K): Option<K>
```

## `floor_key`

The largest key `<= key`, or `None` if every key is larger.

```dream
public fun floor_key(borrow key: K): Option<K>
```

## `keys`

All keys in ascending order.

```dream
public fun keys(): K[]
```

## `values`

All values, ordered by ascending key.

```dream
public fun values(): V[]
```

## `iterator`

Gets an iterator over entries in ascending key order.

```dream
public fun iterator(): SortedMapIterator<K, V>
```
