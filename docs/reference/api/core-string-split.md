# StringSplit

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
