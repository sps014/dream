# Bool

No import is needed.

Read the [usage guide](../language/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `to_int`

1 for true, 0 for false.

```dream
public fun to_int(): int
```

## `parse`

Parses `true` or `false` (case-sensitive, matching language literals).

```dream
public static fun parse(borrow text: string): Result<bool, ParseError>
```
