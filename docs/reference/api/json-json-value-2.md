# JsonKind, JsonValue

**Import:** `import system.json;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](json-json-value.md)

## `as_map`

```dream
public fun as_map(): Option<Map<string, JsonValue>>
```

## `str_or`

```dream
public fun str_or(fallback: string): string
```

## `int_or`

```dream
public fun int_or(fallback: int): int
```

## `bool_or`

```dream
public fun bool_or(fallback: bool): bool
```

## `double_or`

```dream
public fun double_or(fallback: double): double
```

## `get_str`

```dream
public fun get_str(borrow key: string): Option<string>
```

## `get_int`

```dream
public fun get_int(borrow key: string): Option<int>
```

## `get_bool`

```dream
public fun get_bool(borrow key: string): Option<bool>
```

## `get_str_or`

```dream
public fun get_str_or(borrow key: string, fallback: string): string
```

## `get_int_or`

```dream
public fun get_int_or(borrow key: string, fallback: int): int
```

## `get_or`

```dream
public fun get_or(borrow key: string, fallback: JsonValue): JsonValue
```

## `remove`

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

## `get`

```dream
public fun get(borrow key: string): Option<JsonValue>
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

## `set`

```dream
public fun set(key: string, value: JsonValue): void
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
