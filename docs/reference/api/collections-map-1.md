# Map

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections/map.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](collections-map.md)

## `class Map<K, V> : Collection<KeyValuePair<K, V>>`

A generic open-addressing hash map. Slots are packed `{key, value, state}` records.

```dream
public class Map<K, V> : Collection<KeyValuePair<K, V>>
```

## `constructor`

```dream
public constructor(capacity: int = 8)
```

## `length`

```dream
public get length(): int
```

## `capacity`

```dream
public get capacity(): int
```

## `this`

The first probe hits an empty or matching slot on the usual insert. That case stays in the caller; collisions fall through to the full probe.

```dream
public fun this[key: K] = value: V
```

## `set`

```dream
public fun set(key: K, value: V): void
```

## `contains`

```dream
public fun contains(borrow key: K): bool
```

## `set_all`

```dream
public fun set_all(borrow keys: K[], borrow values: V[]): void
```

## `from_arrays`

```dream
public static fun from_arrays(borrow keys: K[], borrow values: V[]): Map<K, V>
```

## `get`

Value for `key`, or `None` when absent.

```dream
public fun get(borrow key: K): Option<V>
```

## `get`

Value for the `string` key equal to `key`, without building a `string`.

```dream
public fun get(key: StringSpan): Option<V> where K : StringKey
```

## `get_or`

```dream
public fun get_or(key: StringSpan, fallback: V): V where K : StringKey
```

## `contains`

```dream
public fun contains(key: StringSpan): bool where K : StringKey
```

## `this`

Value for `key`. Panics when the key is absent — this is what `m[k]` compiles to; use `get(k)` for absence-tolerant reads.

```dream
public fun this[borrow key: K]: V
```

## `get_or`

```dream
public fun get_or(borrow key: K, fallback: V): V
```

## `get_or_insert`

```dream
public fun get_or_insert(key: K, borrow factory: fun(): V): V
```

## `merge`

```dream
public fun merge(borrow other: Map<K, V>): void
```

## `entries`

```dream
public fun entries(): List<KeyValuePair<K, V>>
```

## `remove`

```dream
public fun remove(borrow key: K): bool
```

## `is_empty`

```dream
public fun is_empty(): bool
```

## `clear`

```dream
public fun clear(): void
```

## `clear`

```dream
public fun clear(): void where K : unmanaged, V : unmanaged
```

## `keys`

```dream
public fun keys(): K[]
```

## `values`

```dream
public fun values(): V[]
```
