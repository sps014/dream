# Math

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Math`

Mathematical functions. Trig and most transcendentals are host-native (`env.*`); thin public wrappers expose them (Dream forbids `public` + `extern` on one symbol).

```dream
public static class Math
```

## `abs`

```dream
public static fun abs(x: double): double
```

## `floor`

```dream
public static fun floor(x: double): double
```

## `ceil`

```dream
public static fun ceil(x: double): double
```

## `round`

```dream
public static fun round(x: double): double
```

## `sqrt`

Square root. A negative input is NaN, same as `log` / `asin` and as libm.

```dream
public static fun sqrt(x: double): double
```

## `pow`

```dream
public static fun pow(base: double, exponent: double): double
```

## `sin`

```dream
public static fun sin(x: double): double
```

## `cos`

```dream
public static fun cos(x: double): double
```

## `tan`

```dream
public static fun tan(x: double): double
```

## `asin`

```dream
public static fun asin(x: double): double
```

## `acos`

```dream
public static fun acos(x: double): double
```

## `atan`

```dream
public static fun atan(x: double): double
```

## `atan2`

```dream
public static fun atan2(y: double, x: double): double
```

## `log`

```dream
public static fun log(x: double): double
```

## `log10`

```dream
public static fun log10(x: double): double
```

## `exp`

```dream
public static fun exp(x: double): double
```

## `hypot`

```dream
public static fun hypot(x: double, y: double): double
```

## `PI`

```dream
public static get PI(): double
```

## `E`

```dream
public static get E(): double
```

## `min`

```dream
public static fun min(a: double, b: double): double
```

## `max`

```dream
public static fun max(a: double, b: double): double
```

## `clamp`

```dream
public static fun clamp(x: double, lo: double, hi: double): double
```
