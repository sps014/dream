# JsonValue

**Import:** `import system.json;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `get_bool`

Returns the named boolean property, or `None` if missing or not a boolean.

```dream
public fun get_bool(borrow key: string): Option<bool>
```

## `get_str_or`

Returns the named string property, or `fallback` if missing or not a string.

```dream
public fun get_str_or(borrow key: string, fallback: string): string
```

## `get_int_or`

Returns the named integer property, or `fallback` if missing or not numeric.

```dream
public fun get_int_or(borrow key: string, fallback: int): int
```

## `get_or`

Returns the named property, or `fallback` when absent or not an object.

```dream
public fun get_or(borrow key: string, fallback: JsonValue): JsonValue
```

## `remove`

Removes the named object property; returns whether it existed.

```dream
public fun remove(borrow key: string): bool
```

## `has`

True if the object has the given key.

```dream
public fun has(borrow key: string): bool
```

## `this`

The value for `key`, or `None` when this is not an object or the key is absent.

```dream
public fun this[borrow key: string]: Option<JsonValue>
```

## `at`

The value for `key`. Panics when this is not an object or the key is absent — use `get(key)` for absence-tolerant reads.

```dream
public fun at(borrow key: string): JsonValue
```

## `this`

Sets the value for `key` (objects only).

```dream
public fun this[key: string] = value: JsonValue
```

## `key_at`

The object key at `index`, or `None`.

```dream
public fun key_at(index: int): Option<string>
```

## `value_at`

The object value at insertion index `index`, or `None`.

```dream
public fun value_at(index: int): Option<JsonValue>
```

## `keys`

The keys of the object (empty list when not an object).

```dream
public fun keys(): List<string>
```

## `at`

The array element at `index`, or `None`.

```dream
public fun at(index: int): Option<JsonValue>
```

## `push`

Appends a value to the array.

```dream
public fun push(value: JsonValue): void
```

## `length`

Element count for arrays/objects (0 for scalars).

```dream
public get length(): int
```
