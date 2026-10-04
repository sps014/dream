# JsonParser

**Import:** `import system.json;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class JsonParser`

Code-unit cursor recursive-descent JSON parser over the input string (`char_at` / `.length`) — no intermediate `byte[]` copy.

```dream
public class JsonParser
```

## `constructor`

```dream
public constructor(text: string, pos: int)
```

## `skip_ws`

```dream
public fun skip_ws(): void
```

## `at_end`

```dream
public fun at_end(): bool
```

## `keyword`

```dream
public fun keyword(length: int, value: JsonValue): JsonValue
```

## `parse_string`

```dream
public fun parse_string(): string
```

## `parse_number`

```dream
public fun parse_number(): JsonValue
```

## `parse_array`

```dream
public fun parse_array(): JsonValue
```

## `parse_object`

```dream
public fun parse_object(): JsonValue
```

## `parse_value`

```dream
public fun parse_value(): JsonValue
```

## `try_consume`

```dream
public fun try_consume(code: int): bool
```

## `try_null`

```dream
public fun try_null(): bool
```

## `parse_bool`

```dream
public fun parse_bool(): bool
```

## `parse_int`

```dream
public fun parse_int(): int
```

## `parse_double`

```dream
public fun parse_double(): double
```

## `match_key`

True when the next object key is `key` (no escapes). Consumes the key and the following colon.

```dream
public fun match_key(borrow key: string): bool
```

## `skip_value`

```dream
public fun skip_value(): void
```
