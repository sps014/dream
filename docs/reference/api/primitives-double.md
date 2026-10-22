# Double

No import is needed.

Read the [usage guide](../../language/types/primitives.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `compare`

Orders this value against `other`.

```dream
public fun compare(other: double): int
```

## `abs`

Returns the magnitude of this value with the sign cleared, so `-0.0` becomes `0.0` and NaN stays NaN.

```dream
public fun abs(): double
```

## `min`

Returns the smaller of this value and `other`. When either is NaN, returns `other`.

```dream
public fun min(other: double): double
```

## `max`

Returns the larger of this value and `other`. When either is NaN, returns `other`.

```dream
public fun max(other: double): double
```

## `clamp`

Returns this value limited to the inclusive range `lo` through `hi`. NaN is returned unchanged; the result is unspecified when `lo` is greater than `hi`.

```dream
public fun clamp(lo: double, hi: double): double
```

## `signum`

Returns `1.0` for a positive value, `-1.0` for a negative value, and the value itself for zero (keeping the sign of `-0.0`) or NaN.

```dream
public fun signum(): double
```

## `to_string`

Returns the shortest decimal text that rounds back to exactly this value, so `0.1d` prints as `0.1` and `0.1d + 0.2d` as `0.30000000000000004`. Values from 1e-7 up to but not including 1e21 use plain notation (`0.1`, `3`, `1500000`); smaller and larger magnitudes use an exponent (`1e+21`, `1.5e-8`). NaN is `nan`, the infinities are `inf` and `-inf`, and both zeros are `0`.

```dream
public override fun to_string(): string
```

## `parse`

Parses a decimal double from `text`.

```dream
public static fun parse(borrow text: string): Result<double, ParseError>
```
