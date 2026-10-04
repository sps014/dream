# Float

No import is needed.

Read the [usage guide](../language/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

Orders this value against `other` (negative/zero/positive).

```dream
public fun compare(other: float): int
```

## `abs`

Absolute value of this float.

```dream
public fun abs(): float
```

## `min`

The smaller of this and `other`.

```dream
public fun min(other: float): float
```

## `max`

The larger of this and `other`.

```dream
public fun max(other: float): float
```

## `parse`

Parses a decimal float from `text`.

```dream
public static fun parse(borrow text: string): Result<float, ParseError>
```
