# Float16

No import is needed.

Read the [usage guide](../../language/types/primitives.md#half-precision-float16) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

`float16` is an IEEE 754 binary16 number: 11 significant bits (about 3 decimal digits) and a finite range of ±65504. Write literals with the `h` suffix (`1.5h`). It widens implicitly to `float` and `double`; converting a `float`, a `double`, or an integer to `float16` needs a cast such as `(float16)x`, which rounds to the nearest value (ties to even). Converting to text with `to_string` or string concatenation gives the shortest decimal that parses back to the same value.

## `compare`

Orders this value against `other`: negative when less, zero when equal, positive when greater. NaN compares equal to everything, so sort NaN-free data.

```dream
public fun compare(other: float16): int
```

## `abs`

Returns the absolute value. NaN stays NaN.

```dream
public fun abs(): float16
```

## `min`

Returns the smaller of this value and `other`; `other` when either is NaN.

```dream
public fun min(other: float16): float16
```

## `max`

Returns the larger of this value and `other`; `other` when either is NaN.

```dream
public fun max(other: float16): float16
```

## `clamp`

Returns this value limited to the inclusive range `lo` through `hi`. NaN is returned unchanged; the result is unspecified when `lo` is greater than `hi`.

```dream
public fun clamp(lo: float16, hi: float16): float16
```

## `signum`

Returns `1.0h` for a positive value, `-1.0h` for a negative value, and the value itself for zero (keeping the sign of `-0.0h`) or NaN.

```dream
public fun signum(): float16
```

## `is_nan`

Returns true when this value is NaN (not a number), for example the result of `0.0h / 0.0h`.

```dream
public fun is_nan(): bool
```

## `is_infinite`

Returns true when this value is positive or negative infinity, for example the result of `(float16)70000.0` or `1.0h / 0.0h`.

```dream
public fun is_infinite(): bool
```

## `parse`

Parses decimal text such as `"0.125"`, `"-3"`, or `"6.1e-05"` and rounds it to the nearest `float16`. Magnitudes of 65520 or more become infinity. Also accepts `"nan"`, `"inf"`, `"+inf"`, and `"-inf"`, so every string produced by `to_string` parses back to the same value. Parsing stops at the first character that cannot continue the number; text with no digits fails with a `ParseError`.

```dream
public static fun parse(borrow text: string): Result<float16, ParseError>
```

## `min_value`

The most negative finite `float16`, -65504.

```dream
public static get min_value(): float16
```

## `max_value`

The largest finite `float16`, 65504.

```dream
public static get max_value(): float16
```

## `epsilon`

The gap between 1 and the next larger `float16`: 2^-10, or 0.0009765625. Values near 1 are spaced this far apart; the spacing scales with magnitude.

```dream
public static get epsilon(): float16
```
