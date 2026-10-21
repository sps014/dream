# Math

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Math`

Common mathematical operations on double-precision numbers. Trigonometric arguments and results use radians.

```dream
public static class Math
```

## `floor`

Rounds `x` down to the nearest integer value.

```dream
public static fun floor(x: double): double
```

## `ceil`

Rounds `x` up to the nearest integer value.

```dream
public static fun ceil(x: double): double
```

## `round`

Rounds `x` to the nearest integer value. Halfway cases round away from zero, so `round(2.5)` is `3` and `round(-2.5)` is `-3`.

```dream
public static fun round(x: double): double
```

## `sqrt`

Square root, correctly rounded. A negative input is NaN.

```dream
public static fun sqrt(x: double): double
```

## `pow`

Returns `base` raised to `exponent`. A negative `base` with a non-integer `exponent` is NaN, and `pow(x, 0)` is `1` for every `x`, including NaN.

```dream
public static fun pow(base: double, exponent: double): double
```

## `sin`

Returns the sine of `x`, measured in radians.

```dream
public static fun sin(x: double): double
```

## `cos`

Returns the cosine of `x`, measured in radians.

```dream
public static fun cos(x: double): double
```

## `tan`

Returns the tangent of `x`, measured in radians.

```dream
public static fun tan(x: double): double
```

## `asin`

Returns the inverse sine in radians; inputs outside [-1, 1] produce NaN.

```dream
public static fun asin(x: double): double
```

## `acos`

Returns the inverse cosine in radians; inputs outside [-1, 1] produce NaN.

```dream
public static fun acos(x: double): double
```

## `atan`

Returns the inverse tangent in radians.

```dream
public static fun atan(x: double): double
```

## `atan2`

Returns the angle of the point `(x, y)` in radians, using both signs to choose the quadrant.

```dream
public static fun atan2(y: double, x: double): double
```

## `log`

Returns the natural logarithm of `x`. Zero gives negative infinity and a negative input gives NaN.

```dream
public static fun log(x: double): double
```

## `log10`

Returns the base-ten logarithm of `x`. Zero gives negative infinity and a negative input gives NaN.

```dream
public static fun log10(x: double): double
```

## `exp`

Returns e raised to `x`.

```dream
public static fun exp(x: double): double
```

## `hypot`

Returns the length of the vector `(x, y)` without overflow or underflow in the intermediate squares. An infinite component gives infinity even when the other is NaN.

```dream
public static fun hypot(x: double, y: double): double
```

## `PI`

Ratio of a circle's circumference to its diameter.

```dream
public static get PI(): double
```

## `E`

Base of the natural logarithm.

```dream
public static get E(): double
```
