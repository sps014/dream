# Map

**Import:** `import system.collections;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Map`

A generic open-addressing hash map. Slots are packed `{key, value, state}` records.

```dream
public class Map<K, V>: Collection<KeyValuePair<K, V>>
```

## `constructor`

Creates an empty map, reserving space based on `capacity` (8 by default).

```dream
public constructor(capacity: int = 8)
```

## `reserve`

Reserves additional live entries, accounting for load factor and reclaiming tombstones.

```dream
public fun reserve(additional: int): Result<Unit, CapacityError>
```

## `length`

Number of elements currently stored.

```dream
public get length(): int
```

## `capacity`

Number of elements that fit before storage must grow.

```dream
public get capacity(): int
```

## `this`

The first probe hits an empty or matching slot on the usual insert. That case stays in the caller; collisions fall through to the full probe.

```dream
public fun this[key: K] = value: V
```

## `contains`

Returns whether `key` is present.

```dream
public fun contains(borrow key: K): bool
```

## `set_all`

Inserts corresponding entries from `keys` and `values`; the arrays must have equal lengths.

```dream
public fun set_all(borrow keys: K[], borrow values: V[]): void
```

## `constructor`

Creates a map by pairing keys and values up to the shorter array length; later duplicate keys replace earlier values.

```dream
public constructor(borrow keys: K[], borrow values: V[])
```

## `get`

Value for `key`, or `None` when absent.

```dream
public fun get(borrow key: K): Option<V>
```

## `get`

Value for the `string` key equal to `key`, without building a `string`.

```dream
public fun get(borrow key: ReadOnlySpan<char>): Option<V> where K: StringKey
```

## `contains`

Returns whether `key` is present.

```dream
public fun contains(borrow key: ReadOnlySpan<char>): bool where K: StringKey
```

## `this`

Value for `key`. Panics when the key is absent — this is what `m[k]` compiles to; use `get(k)` for absence-tolerant reads.

```dream
public fun this[borrow key: K]: V
```

## `get_or_insert`

Returns the existing value, or inserts and returns the result of `factory` when absent. The key is borrowed for the lookup and copied only on a miss.

```dream
public fun get_or_insert(borrow key: K, borrow factory: fun(): V): V
```

## `merge`

Copies entries from `other`, replacing values with matching keys.

```dream
public fun merge(borrow other: Map<K, V>): void
```

## `entries`

Returns a new list of key/value pairs.

```dream
public fun entries(): List<KeyValuePair<K, V>>
```

## `remove`

Removes `key` and its value; returns whether the key was present.

```dream
public fun remove(borrow key: K): bool
```

## `is_empty`

Returns whether there are no elements.

```dream
public fun is_empty(): bool
```

## `clear`

Removes all entries while keeping storage for reuse.

```dream
public fun clear(): void
```

## `clear`

Removes all entries while keeping storage for reuse.

```dream
public fun clear(): void where K: unmanaged, V: unmanaged
```

## `keys`

Returns a new list of the keys, in the map's iteration order.

```dream
public fun keys(): List<K>
```

## `values`

Returns a new list of the values, in the map's iteration order.

```dream
public fun values(): List<V>
```

## `iterator`

Creates a cursor over the sequence. Close it when iteration ends early.

```dream
public fun iterator(): MapIterator<K, V>
```

## `to_string`

Returns a text representation of this value.

```dream
public override fun to_string(): string
```
