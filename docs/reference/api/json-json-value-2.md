# JsonValue

**Import:** `import system.json;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
