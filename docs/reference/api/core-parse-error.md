# ParseError

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class ParseError : Error`

Parse / format failure implementing `Error` (JSON, numbers, URLs, ISO-8601, …).

```dream
public class ParseError : Error
```

## `constructor`

Creates an error with machine code and message.

```dream
public constructor(code: string, message: string)
```

## `message`

Human-readable description.

```dream
public fun message(): string
```

## `code`

Stable machine code (typically `EPARSE`).

```dream
public fun code(): string
```

## `invalid`

Generic invalid-input parse failure.

```dream
public static fun invalid(message: string): ParseError
```
