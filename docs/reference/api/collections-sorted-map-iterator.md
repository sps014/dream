# SortedMapIterator

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class SortedMapIterator<K : Comparable<K>, V> : Iterator<KeyValuePair<K, V>>`

Cursor produced by `SortedMap.iterator()` — walks entries in ascending key order.

```dream
public class SortedMapIterator<K : Comparable<K>, V> : Iterator<KeyValuePair<K, V>>
```

## `next`

Returns the next key/value pair in ascending key order, or `None` if exhausted.

```dream
public fun next(): Option<KeyValuePair<K, V>>
```
