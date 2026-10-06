# Json

**Import:** `import system.json;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class Json`

The public JSON API: one pair of names for text <-> value.

```dream
public static class Json
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

```dream
public static fun write_bool(borrow sb: StringBuilder, b: bool): void
```

## `write_null`

```dream
public static fun write_null(borrow sb: StringBuilder): void
```

## `write_value`

Appends an already-built dynamic value as compact JSON text. Generated `write_json` methods use this for `JsonValue` fields and collection elements, which carry their own shape.

```dream
public static fun write_value(borrow sb: StringBuilder, borrow v: JsonValue): void
```

## `write_int`

```dream
public static fun write_int(borrow sb: StringBuilder, n: int): void
```

## `write_number`

```dream
public static fun write_number(borrow sb: StringBuilder, n: double): void
```
