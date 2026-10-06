# StringSpan

No import is needed.

Read the [usage guide](../language/spans.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](core-string-span.md)

## `last_index_of`

Empty `sub` is `Some(length)`, like `string.last_index_of`.

```dream
public fun last_index_of(borrow sub: string): Option<int>
```

## `trim`

The view without leading and trailing whitespace.

```dream
public fun trim(): StringSpan
```

## `trim_start`

```dream
public fun trim_start(): StringSpan
```

## `trim_end`

```dream
public fun trim_end(): StringSpan
```

## `parse_int`

A (possibly signed) decimal `int`, with the same errors as `int.parse`.

```dream
public fun parse_int(): Result<int, ParseError>
```

## `parse_double`

```dream
public fun parse_double(): Result<double, ParseError>
```

## `split_iter`

Pieces between occurrences of `sep`; `while it.move_next() { it.current }`.

```dream
public fun split_iter(sep: char): StringSplit
```

## `split_iter`

```dream
public fun split_iter(borrow sep: string): StringSplit
```

## `lines`

Lines split on `\n` with a trailing `\r` dropped; a final `\n` does not start a new line.

```dream
public fun lines(): StringLines
```

## `ref struct StringSplit`

Cursor over the pieces of a `StringSpan` between separators. Every piece is a view of the same source; nothing is allocated.

```dream
public ref struct StringSplit
```

## `move_next`

Advances to the next piece; `false` once every piece has been produced.

```dream
public fun move_next(): bool
```

## `current`

The piece produced by the last `move_next()`.

```dream
public get current(): StringSpan
```

## `ref struct StringLines`

Cursor over the lines of a `StringSpan`.

```dream
public ref struct StringLines
```

## `move_next`

```dream
public fun move_next(): bool
```

## `current`

```dream
public get current(): StringSpan
```

## `interface StringKey`

A `Map`/`Set` key that can be found by a `StringSpan` without building a `string`.

```dream
public interface StringKey
```

## `matches_span`

```dream
fun matches_span(other: StringSpan): bool
```

## `span`

A view of the whole string.

```dream
public fun span(): StringSpan
```

## `span`

A view of `[start, end)`, clamped exactly like `substring`.

```dream
public fun span(start: int, end: int): StringSpan
```

## `matches_span`

```dream
public fun matches_span(other: StringSpan): bool
```
