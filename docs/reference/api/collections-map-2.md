# Map

**Import:** `import system.collections;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
