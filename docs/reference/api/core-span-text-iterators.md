# Text iterators and string views

No import is needed.

Read the [span guide](../../language/types/spans.md) for usage and backing-storage semantics.

## `StringSplit`

Cursor over the pieces of a `ReadOnlySpan<char>` between separators. Every piece is a view of the same source; nothing is allocated.

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
public get current(): ReadOnlySpan<char>
```

## `StringLines`

Cursor over the lines of a `ReadOnlySpan<char>`.

```dream
public ref struct StringLines
```

## `move_next`

Advances to the next segment; returns false at the end.

```dream
public fun move_next(): bool
```

## `current`

View of the segment selected by the last successful `move_next`.

```dream
public get current(): ReadOnlySpan<char>
```

## `StringKey`

A `Map`/`Set` key that can be found by a `ReadOnlySpan<char>` without building a `string`.

```dream
public interface StringKey
```

## `matches_span`

Returns whether this value equals the supplied text view.

```dream
fun matches_span(borrow other: ReadOnlySpan<char>): bool
```

## `span`

A view of the whole string.

```dream
public fun span(): ReadOnlySpan<char>
```

## `span`

A view of `[start, end)`, clamped exactly like `substring`.

```dream
public fun span(start: int, end: int): ReadOnlySpan<char>
```

## `matches_span`

Returns whether this value equals the supplied text view.

```dream
public fun matches_span(borrow other: ReadOnlySpan<char>): bool
```

## `view_eq`

Tests equality of the supplied text ranges.

```dream
public static extern fun view_eq(
    borrow a: string,
    ao: int,
    an: int,
    borrow b: string,
    bo: int,
    bn: int
): int
```

## `view_compare`

Compares the supplied text ranges in lexical order.

```dream
public static extern fun view_compare(
    borrow a: string,
    ao: int,
    an: int,
    borrow b: string,
    bo: int,
    bn: int
): int
```

## `view_hash`

Returns a hash of the specified text range.

```dream
public static extern fun view_hash(borrow s: string, off: int, n: int): int
```

## `view_find`

Finds the first matching text range at or after `from`, or returns -1.

```dream
public static extern fun view_find(
    borrow h: string,
    ho: int,
    hn: int,
    borrow nd: string,
    no: int,
    nn: int,
    from: int
): int
```

## `view_rfind`

Finds the last matching text range, or returns -1.

```dream
public static extern fun view_rfind(
    borrow h: string,
    ho: int,
    hn: int,
    borrow nd: string,
    no: int,
    nn: int
): int
```

## `view_find_char`

Finds character `c` at or after `from`, or returns -1.

```dream
public static extern fun view_find_char(
    borrow s: string,
    off: int,
    n: int,
    from: int,
    c: int
): int
```

## `view_rfind_char`

Finds the last occurrence of character `c`, or returns -1.

```dream
public static extern fun view_rfind_char(borrow s: string, off: int, n: int, c: int): int
```

## `view_count_char`

Counts occurrences of character `c` in the specified text range.

```dream
public static extern fun view_count_char(borrow s: string, off: int, n: int, c: int): int
```
