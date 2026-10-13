# Text iterators and string views

No import is needed.

Read the [span guide](../language/spans.md) for usage and backing-storage semantics.

## `StringSplit`

Cursor over the pieces of a `ReadOnlySpan<char>` between separators. Every piece is a view of the same source; nothing is allocated.

```dream
public ref struct StringSplit
```

## `move_next`

```dream
public fun move_next(): bool
```

## `current`

```dream
public get current(): ReadOnlySpan<char>
```

## `StringLines`

Cursor over the lines of a `ReadOnlySpan<char>`.

```dream
public ref struct StringLines
```

## `move_next`

```dream
public fun move_next(): bool
```

## `current`

```dream
public get current(): ReadOnlySpan<char>
```

## `StringKey`

A `Map`/`Set` key that can be found by a `ReadOnlySpan<char>` without building a `string`.

```dream
public interface StringKey
```

## `matches_span`

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

```dream
public fun matches_span(borrow other: ReadOnlySpan<char>): bool
```

## `view_eq`

Compares two UTF-16 ranges for equality without allocating strings.

```dream
public static extern fun view_eq(borrow a: string, ao: int, an: int, borrow b: string, bo: int, bn: int): int
```

## `view_compare`

Lexicographically compares two UTF-16 ranges, returning a negative, zero, or positive result.

```dream
public static extern fun view_compare(borrow a: string, ao: int, an: int, borrow b: string, bo: int, bn: int): int
```

## `view_hash`

Hashes a UTF-16 range without allocating a string.

```dream
public static extern fun view_hash(borrow s: string, off: int, n: int): int
```

## `view_find`

Finds a substring in a UTF-16 range starting at `from`; returns a relative index or -1.

```dream
public static extern fun view_find(borrow h: string, ho: int, hn: int, borrow nd: string, no: int, nn: int, from: int): int
```

## `view_rfind`

Finds the last substring occurrence in a UTF-16 range; returns a relative index or -1.

```dream
public static extern fun view_rfind(borrow h: string, ho: int, hn: int, borrow nd: string, no: int, nn: int): int
```

## `view_find_char`

Finds code unit `c` in a UTF-16 range starting at `from`; returns a relative index or -1.

```dream
public static extern fun view_find_char(borrow s: string, off: int, n: int, from: int, c: int): int
```

## `view_rfind_char`

Finds the last occurrence of code unit `c` in a UTF-16 range; returns a relative index or -1.

```dream
public static extern fun view_rfind_char(borrow s: string, off: int, n: int, c: int): int
```

## `view_count_char`

Counts occurrences of code unit `c` in a UTF-16 range.

```dream
public static extern fun view_count_char(borrow s: string, off: int, n: int, c: int): int
```
