# JsonValue

**Import:** `import system.json;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsonValue`

A JSON value: null, boolean, number, string, array, or object. Arrays and objects can contain nested values. Object properties retain insertion order when serialized.

```dream
public class JsonValue
```

## `null`

Creates a JSON `null`.

```dream
public static fun null(): JsonValue
```

## `from_bool`

Creates a JSON boolean holding `b`.

```dream
public static fun from_bool(b: bool): JsonValue
```

## `from_double`

Creates a JSON number holding `n`. Integral values serialize without a decimal point.

```dream
public static fun from_double(n: double): JsonValue
```

## `from_int`

Creates a JSON number holding `n`; every `int` is represented exactly.

```dream
public static fun from_int(n: int): JsonValue
```

## `from_string`

Creates a JSON string holding `s`.

```dream
public static fun from_string(s: string): JsonValue
```

## `array`

Creates an empty JSON array; add elements with `push`.

```dream
public static fun array(): JsonValue
```

## `dict`

Creates an empty JSON object; add properties with `value[key] = item`. Properties serialize in insertion order.

```dream
public static fun dict(): JsonValue
```

## `kind`

The kind of JSON value, for `switch (v.kind()) { case JsonKind.Object: … }`.

```dream
public fun kind(): JsonKind
```

## `is_null`

True if the value is null.

```dream
public fun is_null(): bool
```

## `is_array`

True if the value is an array.

```dream
public fun is_array(): bool
```

## `is_object`

True if the value is an object.

```dream
public fun is_object(): bool
```

## `as_bool`

Returns the value as a bool when this is a JSON boolean.

```dream
public fun as_bool(): Option<bool>
```

## `as_double`

Returns the value as a double when this is a JSON number.

```dream
public fun as_double(): Option<double>
```

## `as_int`

Returns the value as an integer when this is a JSON number.

```dream
public fun as_int(): Option<int>
```

## `as_string`

Returns the value as a string when this is a JSON string.

```dream
public fun as_string(): Option<string>
```

## `as_array`

Returns the elements when this is an array, otherwise `None`.

```dream
public fun as_array(): Option<List<JsonValue>>
```

## `as_map`

Returns the entries when this is an object, otherwise `None`.

```dream
public fun as_map(): Option<Map<string, JsonValue>>
```

## `remove`

Removes the named object property; returns whether it existed.

```dream
public fun remove(borrow key: string): bool
```

## `contains`

Returns whether this is an object with a property named `key`. Always `false` for arrays and scalars.

```dream
public fun contains(borrow key: string): bool
```

## `get`

Returns the property named `key`, or `None` when this is not an object or the property is absent. Chain `as_string`, `as_int`, and similar readers through `and_then` to read a typed property.

```dream
public fun get(borrow key: string): Option<JsonValue>
```

## `this`

Returns the property named `key`. Panics when this is not an object or the property is absent; use `get` when absence is expected.

```dream
public fun this[borrow key: string]: JsonValue
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
