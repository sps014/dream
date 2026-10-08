# Text iterators and string views

No import is needed.

Read the [span guide](../language/spans.md) for usage and backing-storage semantics.

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
fun matches_span(other: ReadOnlySpan<char>): bool
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
public fun matches_span(other: ReadOnlySpan<char>): bool
```
