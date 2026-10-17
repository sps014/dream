# StringKey

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
