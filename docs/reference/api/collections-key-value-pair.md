# KeyValuePair

**Import:** `import system.collections;`

Read the [usage guide](../stdlib/collections.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct KeyValuePair<K, V>`

One key/value entry (value type — no heap allocation of the pair itself).

```dream
public struct KeyValuePair<K, V>
```

## `key: K`

```dream
public key: K
```

## `value: V`

```dream
public value: V
```

## `constructor`

Builds a pair from `key` and `value`.

```dream
public constructor(key: K, value: V)
```
