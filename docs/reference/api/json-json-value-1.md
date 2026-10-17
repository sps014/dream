# JsonValue

**Import:** `import system.json;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsonValue`

A JSON value: null, boolean, number, string, array, or object. Arrays and objects can contain nested values. Object properties retain insertion order when serialized.

```dream
public class JsonValue
```

## `none`

A new null JSON value.

```dream
public static fun none(): JsonValue
```

## `boolean`

A new boolean JSON value.

```dream
public static fun boolean(b: bool): JsonValue
```

## `number`

A new number JSON value.

```dream
public static fun number(n: double): JsonValue
```

## `from_int`

A new number JSON value from int.

```dream
public static fun from_int(n: int): JsonValue
```

## `from_string`

A new string JSON value.

```dream
public static fun from_string(s: string): JsonValue
```

## `array`

A new empty JSON array (allocates only the items list).

```dream
public static fun array(): JsonValue
```

## `dict`

A new empty JSON object (allocates keys, parallel values, and map).

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

## `str_or`

Returns the string value, or `fallback` for a different JSON kind.

```dream
public fun str_or(fallback: string): string
```

## `int_or`

Returns the integer value, or `fallback` for a different JSON kind.

```dream
public fun int_or(fallback: int): int
```

## `bool_or`

Returns the boolean value, or `fallback` for a different JSON kind.

```dream
public fun bool_or(fallback: bool): bool
```

## `double_or`

Returns the numeric value, or `fallback` for a different JSON kind.

```dream
public fun double_or(fallback: double): double
```

## `get_str`

Returns the named string property, or `None` if missing or not a string.

```dream
public fun get_str(borrow key: string): Option<string>
```

## `get_int`

Returns the named integer property, or `None` if missing or not numeric.

```dream
public fun get_int(borrow key: string): Option<int>
```
