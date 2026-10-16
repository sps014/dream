# Map

**Import:** `import system.collections;`

## `values`

Returns a new array containing the values.

```dream
public fun values(): V[]
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
