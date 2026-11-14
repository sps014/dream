# GenWire

**Import:** `import system.codegen;`

Read the [usage guide](../../language/metaprogramming/generators.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `GenWire`

One value in the generator snapshot and result codec.

```dream
public class GenWire
```

## `null`

A null value.

```dream
public static fun null(): GenWire
```

## `dict`

An empty object.

```dream
public static fun dict(): GenWire
```

## `array`

An empty array.

```dream
public static fun array(): GenWire
```

## `from_string`

A string value.

```dream
public static fun from_string(text: string): GenWire
```

## `from_int`

An integer value.

```dream
public static fun from_int(n: int): GenWire
```

## `from_bool`

A boolean value.

```dream
public static fun from_bool(flag: bool): GenWire
```

## `is_null`

Whether this value is null.

```dream
public fun is_null(): bool
```

## `as_string`

This string, when the value is a string.

```dream
public fun as_string(): Option<string>
```

## `as_int`

This integer, when it is an integer that fits in `int`.

```dream
public fun as_int(): Option<int>
```

## `as_bool`

This boolean, when the value is a boolean.

```dream
public fun as_bool(): Option<bool>
```

## `as_array`

This array's elements, when the value is an array.

```dream
public fun as_array(): Option<List<GenWire>>
```

## `get`

The property named `key`, or `None` when this is not an object or `key` is absent.

```dream
public fun get(borrow key: string): Option<GenWire>
```

## `this`

The property named `key`. Absent keys are null.

```dream
public fun this[borrow key: string]: GenWire
```

## `this`

Sets `key` on an object, appending it when it is new.

```dream
public fun this[key: string] = value: GenWire
```

## `push`

Appends `value` to an array.

```dream
public fun push(value: GenWire): void
```

## `text`

Text used when an option is not itself a string: decimals, `true`/`false`, and `null`.

```dream
public fun text(): string
```

## `decode`

Reads a `DGEN` payload.

```dream
public static fun decode(borrow bytes: byte[]): Result<GenWire, string>
```

## `encode`

Writes this value as a `DGEN` payload.

```dream
public fun encode(): byte[]
```
