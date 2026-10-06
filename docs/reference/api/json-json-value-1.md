# JsonKind, JsonValue

**Import:** `import system.json;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](json-json-value.md)

## `enum JsonKind`

Discriminant for `JsonValue.kind()`.

```dream
public enum JsonKind
```

## `Null`

```dream
Null
```

## `Bool`

```dream
Bool
```

## `Number`

```dream
Number
```

## `String`

```dream
String
```

## `Array`

```dream
Array
```

## `Object`

```dream
Object
```

## `class JsonValue`

Native JSON value tree. `items` / `obj_map` can hold nested `JsonValue`s; trees are acyclic at runtime, so `@allow_cycle` opts out of the structural self-edge check.  Container fields are `Option` so scalars allocate no List/Map. Arrays own `items` only; objects own `obj_keys` + `obj_vals` (insertion order) + `obj_map` (O(1) lookup).

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

```dream
public fun as_array(): Option<List<JsonValue>>
```
