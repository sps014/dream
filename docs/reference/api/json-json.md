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

Pretty-prints a `JsonValue` with newlines and `indent` spaces per nesting level. Pass 0 to produce compact JSON text.

```dream
public static fun serialize_pretty(borrow value: JsonValue, indent: int): string
```
