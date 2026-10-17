# StringLines

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
