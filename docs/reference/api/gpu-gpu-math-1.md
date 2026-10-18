# GpuMath

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

This type spans several pages: [part 1](gpu-gpu-math-1.md) · [part 2](gpu-gpu-math-2.md).

## `GpuMath`

Math functions for shaders. Each works on `float`, `float16`, `GpuVec2`, `GpuVec3` and `GpuVec4`, component by component, and returns the same type as its arguments. All of them except `step`, `sign`, `floor`, `ceil`, `round` and `fract` are differentiable inside `@differentiable` helpers. They exist only on the GPU; on the CPU use `Math` and the vector methods.

```dream
public static class GpuMath
```

## `sin`

Sine of `x` radians.

```dream
public static extern fun sin<T>(x: T): T
```

## `cos`

Cosine of `x` radians.

```dream
public static extern fun cos<T>(x: T): T
```

## `tan`

Tangent of `x` radians.

```dream
public static extern fun tan<T>(x: T): T
```

## `asin`

Arcsine in radians, for `x` from `-1` to `1`.

```dream
public static extern fun asin<T>(x: T): T
```

## `acos`

Arccosine in radians, for `x` from `-1` to `1`.

```dream
public static extern fun acos<T>(x: T): T
```

## `atan`

Arctangent in radians.

```dream
public static extern fun atan<T>(x: T): T
```

## `atan2`

The angle in radians of the point `(x, y)`, from `-π` to `π`.

```dream
public static extern fun atan2<T>(y: T, x: T): T
```

## `exp`

`e` raised to `x`.

```dream
public static extern fun exp<T>(x: T): T
```

## `exp2`

`2` raised to `x`.

```dream
public static extern fun exp2<T>(x: T): T
```

## `log`

Natural logarithm, for positive `x`.

```dream
public static extern fun log<T>(x: T): T
```

## `log2`

Base-2 logarithm, for positive `x`.

```dream
public static extern fun log2<T>(x: T): T
```

## `pow`

`x` raised to `y`, for non-negative `x`.

```dream
public static extern fun pow<T>(x: T, y: T): T
```

## `sqrt`

Square root, for non-negative `x`.

```dream
public static extern fun sqrt<T>(x: T): T
```

## `rsqrt`

`1 / sqrt(x)`, usually faster than dividing.

```dream
public static extern fun rsqrt<T>(x: T): T
```

## `abs`

Absolute value.

```dream
public static extern fun abs<T>(x: T): T
```

## `sign`

`-1`, `0` or `1` by the sign of `x`.

```dream
public static extern fun sign<T>(x: T): T
```

## `floor`

Rounds down to a whole number.

```dream
public static extern fun floor<T>(x: T): T
```

## `ceil`

Rounds up to a whole number.

```dream
public static extern fun ceil<T>(x: T): T
```

## `round`

Rounds to the nearest whole number, halves to even.

```dream
public static extern fun round<T>(x: T): T
```

## `fract`

`x - floor(x)`, from `0` up to but excluding `1`.

```dream
public static extern fun fract<T>(x: T): T
```

## `min`

The smaller of `a` and `b`.

```dream
public static extern fun min<T>(a: T, b: T): T
```

## `max`

The larger of `a` and `b`.

```dream
public static extern fun max<T>(a: T, b: T): T
```

## `clamp`

`x` limited to the range `low` through `high`.

```dream
public static extern fun clamp<T>(x: T, low: T, high: T): T
```
