# Json

**Import:** `import system.json;`

Read the [usage guide](../../library/serialization/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Json`

The public JSON API: one pair of names for text <-> value.

```dream
public static class Json
```

## `serialize`

Encodes `value` as JSON text. User-defined types need `@json`.

```dream
public static extern fun serialize<T>(borrow value: T): string
```

## `deserialize`

Decodes JSON text as `T`, returning `ParseError` if it cannot be decoded. User-defined types need `@json`.

```dream
public static extern fun deserialize<T>(borrow text: string): Result<T, ParseError>
```

## `from_value`

Reconstruct a `@json` value from an already-parsed `JsonValue` (no parse step).

```dream
public static extern fun from_value<T>(borrow value: JsonValue): T
```

## `serialize_pretty`

Pretty-prints a `JsonValue` with newlines and `indent` spaces per nesting level. Pass 0 to fall back to compact `_stringify`.

```dream
public static fun serialize_pretty(borrow value: JsonValue, indent: int): string
```

## `_stringify`

Compact JSON text of a dynamic `JsonValue` tree (used by `serialize` for `JsonValue`).

```dream
public static fun _stringify(borrow value: JsonValue): string
```

## `_parse`

Parses `text` into a dynamic `JsonValue` tree (used by `deserialize<JsonValue>`).

```dream
public static fun _parse(borrow text: string): Result<JsonValue, ParseError>
```

## `write_string`

--- serialization helpers (also used by `@json` `write_json` methods) -------------------

```dream
public static fun write_string(borrow sb: StringBuilder, borrow s: string): void
```

## `write_bool`

Appends a JSON boolean to `sb`.

```dream
public static fun write_bool(borrow sb: StringBuilder, b: bool): void
```

## `write_null`

Appends JSON null to `sb`.

```dream
public static fun write_null(borrow sb: StringBuilder): void
```

## `write_value`

Appends a dynamic JSON value to `sb` as compact JSON text.

```dream
public static fun write_value(borrow sb: StringBuilder, borrow v: JsonValue): void
```

## `write_int`

Appends a JSON integer to `sb`.

```dream
public static fun write_int(borrow sb: StringBuilder, n: int): void
```

## `write_number`

Appends a JSON number to `sb`.

```dream
public static fun write_number(borrow sb: StringBuilder, n: double): void
```
