# KeyValuePair

**Import:** `import system.collections;`

Read the [usage guide](../../library/collections/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `KeyValuePair`

One key/value entry (value type — no heap allocation of the pair itself).

```dream
public struct KeyValuePair<K, V>
```

## `key`

The key under which `value` is stored.

```dream
public key: K
```

## `value`

The value stored under `key`.

```dream
public value: V
```

## `constructor`

Builds a pair from `key` and `value`.

```dream
public constructor(key: K, value: V)
```
