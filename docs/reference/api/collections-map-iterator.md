# MapIterator

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class MapIterator<K, V> : Iterator<KeyValuePair<K, V>>`

Cursor produced by `Map.iterator()` — walks occupied slots in place (no snapshot).

```dream
public class MapIterator<K, V> : Iterator<KeyValuePair<K, V>>
```

## `next`

Returns the next key/value pair, or `None` if exhausted.

```dream
public fun next(): Option<KeyValuePair<K, V>>
```
