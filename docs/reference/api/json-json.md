# Json

**Import:** `import system.json;`

Read the [usage guide](../../library/serialization/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Json`

The public JSON API: one pair of names for text <-> value.

```dream
public static class Json
```

## `serialize`

Encodes `value` as compact JSON text, with field names as declared. User-defined types need `@json`.

```dream
public static fun serialize<T>(borrow value: T): string
```

## `serialize`

Encodes `value` as JSON text using `options` for property naming, whether `None` fields are written, and indentation. User-defined types need `@json`.

```dream
public static fun serialize<T>(borrow value: T, borrow options: JsonOptions): string
```

## `deserialize`

Decodes JSON text as `T`, returning `ParseError` if it cannot be decoded. Property names must match field names exactly. User-defined types need `@json`.

```dream
public static fun deserialize<T>(borrow text: string): Result<T, ParseError>
```

## `deserialize`

Decodes JSON text as `T`, matching property names to fields as `options` describes. Returns `ParseError` if the text cannot be decoded. User-defined types need `@json`.

```dream
public static fun deserialize<T>(
    borrow text: string,
    borrow options: JsonOptions
): Result<T, ParseError>
```

## `from_value`

Builds a `@json` value from an already-parsed `JsonValue`. Missing or mismatched properties take the field defaults.

```dream
public static fun from_value<T>(borrow value: JsonValue): T
```

## `from_value`

Builds a `@json` value from an already-parsed `JsonValue`, matching property names to fields as `options` describes. Missing or mismatched properties take the field defaults.

```dream
public static fun from_value<T>(borrow value: JsonValue, borrow options: JsonOptions): T
```
