# JsonParser

**Import:** `import system.json;`

Read the [usage guide](../../library/serialization/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsonParser`

Code-unit cursor recursive-descent JSON parser over the input string (`char_at` / `.length`) — no intermediate `byte[]` copy.

```dream
public class JsonParser
```

## `constructor`

Creates a `JsonParser` with `text`, `pos`.

```dream
public constructor(text: string, pos: int)
```

## `skip_ws`

Advances past JSON whitespace.

```dream
public fun skip_ws(): void
```

## `at_end`

Returns whether the cursor has reached the end of the input.

```dream
public fun at_end(): bool
```

## `cursor`

Current zero-based character position in the input.

```dream
public fun cursor(): int
```

## `failed`

Returns whether parsing has encountered an error.

```dream
public fun failed(): bool
```

## `container_end`

Checks whether a JSON container has closed. Marks parsing as failed if a reader neither consumed input nor reached the closing delimiter.

```dream
public fun container_end(close: int, previous: int): bool
```

## `keyword`

Consumes a recognized keyword of `length` characters and returns its JSON value.

```dream
public fun keyword(length: int, value: JsonValue): JsonValue
```

## `parse_string`

Reads a JSON string and advances the cursor; malformed input marks parsing as failed.

```dream
public fun parse_string(): string
```

## `parse_number`

Reads a JSON number and advances the cursor; malformed input marks parsing as failed.

```dream
public fun parse_number(): JsonValue
```

## `parse_array`

Reads a JSON array and advances the cursor; malformed input marks parsing as failed.

```dream
public fun parse_array(): JsonValue
```

## `parse_object`

Reads a JSON object and advances the cursor; malformed input marks parsing as failed.

```dream
public fun parse_object(): JsonValue
```

## `parse_value`

Reads the next JSON value and advances the cursor; malformed input marks parsing as failed.

```dream
public fun parse_value(): JsonValue
```

## `try_consume`

Consumes the next character if its code equals `code`, returning whether it matched.

```dream
public fun try_consume(code: int): bool
```

## `try_null`

Consumes a JSON null at the cursor, returning whether it matched.

```dream
public fun try_null(): bool
```

## `parse_bool`

Reads a JSON boolean and advances the cursor; invalid input marks parsing as failed.

```dream
public fun parse_bool(): bool
```

## `parse_int`

Parses a decimal integer, returning `ParseError` for invalid or out-of-range input.

```dream
public fun parse_int(): int
```

## `parse_double`

Parses a floating-point number, returning `ParseError` for invalid input.

```dream
public fun parse_double(): double
```

## `match_key`

True when the next object key is `key` (no escapes). Consumes the key and the following colon.

```dream
public fun match_key(borrow key: string): bool
```

## `skip_value`

Advances past the next JSON value without retaining it.

```dream
public fun skip_value(): void
```
