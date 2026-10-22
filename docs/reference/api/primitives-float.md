# Float

No import is needed.

Read the [usage guide](../../language/types/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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

## `clamp`

Returns this value limited to the inclusive range `lo` through `hi`. NaN is returned unchanged; the result is unspecified when `lo` is greater than `hi`.

```dream
public fun clamp(lo: float, hi: float): float
```

## `signum`

Returns `1.0` for a positive value, `-1.0` for a negative value, and the value itself for zero (keeping the sign of `-0.0`) or NaN.

```dream
public fun signum(): float
```

## `to_string`

Returns the shortest decimal text that rounds back to exactly this value, so `0.1f` prints as `0.1`. The layout, NaN, infinity, and zero spellings match `double.to_string`.

```dream
public override fun to_string(): string
```

## `parse`

Parses a decimal float from `text`.

```dream
public static fun parse(borrow text: string): Result<float, ParseError>
```
