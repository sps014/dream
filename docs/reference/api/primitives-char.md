# Char

No import is needed.

Read the [usage guide](../language/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

Orders this character against `other` by code point, implementing `Comparable<char>`.

```dream
public fun compare(other: char): int
```

## `is_digit`

True if this character is an ASCII decimal digit ('0'-'9').

```dream
public fun is_digit(): bool
```

## `is_alpha`

True if this character is an ASCII letter ('a'-'z' or 'A'-'Z').

```dream
public fun is_alpha(): bool
```

## `is_whitespace`

True if this character is ASCII whitespace (space, tab, newline, or carriage return).

```dream
public fun is_whitespace(): bool
```

## `to_lower`

The lowercase form of an ASCII uppercase letter; other characters are returned unchanged.

```dream
public fun to_lower(): char
```

## `to_upper`

The uppercase form of an ASCII lowercase letter; other characters are returned unchanged.

```dream
public fun to_upper(): char
```

## `to_int`

This character's numeric code point.

```dream
public fun to_int(): int
```

## `as_string`

A new single-character string containing this character.

```dream
public fun as_string(): string
```

## `parse`

Parses exactly one Unicode scalar from `text`.

```dream
public static fun parse(borrow text: string): Result<char, ParseError>
```
