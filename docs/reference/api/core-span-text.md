# Character view operations

No import is needed.

Read the [span guide](../../language/types/spans.md) for usage and backing-storage semantics.

## `char_at`

Returns the character at the zero-based index; an invalid index panics.

```dream
public fun char_at(index: int): char
```

## `byte_size`

String payloads use UTF-16; character arrays store full 32-bit values.

```dream
public fun byte_size(): int
```

## `byte_at`

Returns the byte at the zero-based byte index; an invalid index panics.

```dream
public fun byte_at(index: int): int
```

## `to_string`

Returns a text representation of this value.

```dream
public override fun to_string(): string
```

## `hash_code`

Matches string hashing for UTF-16 elements without allocating a string.

```dream
public override fun hash_code(): int
```

## `equals`

Returns whether this value equals `other`.

```dream
public fun equals(borrow other: string): bool
```

## `compare`

Returns a negative number, zero, or a positive number for less than, equal to, or greater than `other`.

```dream
public fun compare(other: ReadOnlySpan<T>): int
```

## `compare`

Returns a negative number, zero, or a positive number for less than, equal to, or greater than `other`.

```dream
public fun compare(borrow other: string): int
```

## `starts_with`

Returns whether this text begins with `prefix`.

```dream
public fun starts_with(borrow prefix: string): bool
```

## `ends_with`

Returns whether this text ends with `suffix`.

```dream
public fun ends_with(borrow suffix: string): bool
```

## `trim`

Returns a view without leading or trailing whitespace.

```dream
public fun trim(): ReadOnlySpan<T>
```

## `trim_start`

Returns a view without leading whitespace.

```dream
public fun trim_start(): ReadOnlySpan<T>
```

## `trim_end`

Returns a view without trailing whitespace.

```dream
public fun trim_end(): ReadOnlySpan<T>
```

## `parse_int`

Parses a decimal integer, returning `ParseError` for invalid or out-of-range input.

```dream
public fun parse_int(): Result<int, ParseError>
```

## `parse_double`

Parses a floating-point number, returning `ParseError` for invalid input.

```dream
public fun parse_double(): Result<double, ParseError>
```

## `split_iter`

Iterates over segments separated by `sep` without copying their contents.

```dream
public fun split_iter(sep: char): StringSplit
```

## `split_iter`

Iterates over segments separated by `sep` without copying their contents.

```dream
public fun split_iter(borrow sep: string): StringSplit
```

## `lines`

Iterates over the lines without copying their contents.

```dream
public fun lines(): StringLines
```
