# GpuMath

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-math.md)

## `static class GpuMath`

Math helpers for GPU shaders (`@compute` / `@vertex` / `@fragment`). Inside shaders the WGSL emitter maps these method names to WGSL builtins (`sin`, `normalize`, `cross`, …). On the CPU they compute the same result with host `Math`.

```dream
public static class GpuMath
```

## `min`

Smaller of two floats (WGSL `min`).

```dream
public static fun min(a: float, b: float): float
```

## `max`

Larger of two floats (WGSL `max`).

```dream
public static fun max(a: float, b: float): float
```

## `abs`

Absolute value (WGSL `abs`).

```dream
public static fun abs(a: float): float
```

## `clamp`

Clamps `x` into `[lo, hi]` (WGSL `clamp`).

```dream
public static fun clamp(x: float, lo: float, hi: float): float
```

## `floor`

Greatest integer ≤ `x` as float (WGSL `floor`).

```dream
public static fun floor(x: float): float
```

## `fract`

Fractional part `x - floor(x)` (WGSL `fract`).

```dream
public static fun fract(x: float): float
```

## `ceil`

Smallest integer ≥ `x` as float (WGSL `ceil`).

```dream
public static fun ceil(x: float): float
```

## `sqrt`

Square root. A negative input is `0` here (WGSL `sqrt` is undefined for x < 0); `Math.sqrt` itself returns NaN.

```dream
public static fun sqrt(x: float): float
```

## `inversesqrt`

Reciprocal square root; returns `0` when `sqrt(x)` is zero (WGSL `inverseSqrt`).

```dream
public static fun inversesqrt(x: float): float
```

## `sin`

Sine in radians (WGSL `sin`).

```dream
public static fun sin(x: float): float
```

## `cos`

Cosine in radians (WGSL `cos`).

```dream
public static fun cos(x: float): float
```

## `tan`

Tangent in radians (WGSL `tan`).

```dream
public static fun tan(x: float): float
```

## `asin`

Arcsine in radians (WGSL `asin`).

```dream
public static fun asin(x: float): float
```

## `acos`

Arccosine in radians (WGSL `acos`).

```dream
public static fun acos(x: float): float
```

## `atan`

Arctangent in radians (WGSL `atan`).

```dream
public static fun atan(x: float): float
```

## `atan2`

Two-argument arctangent of `y`/`x` in radians (WGSL `atan2`).

```dream
public static fun atan2(y: float, x: float): float
```

## `log`

Natural log (WGSL `log`).

```dream
public static fun log(x: float): float
```

## `sign`

Sign of `x`: −1, 0, or 1 (WGSL `sign`).

```dream
public static fun sign(x: float): float
```

## `saturate`

Clamps `x` into `[0, 1]` (WGSL `saturate` / `clamp(x, 0, 1)`).

```dream
public static fun saturate(x: float): float
```

## `step`

Heaviside step: `0` if `x < edge`, else `1` (WGSL `step`).

```dream
public static fun step(edge: float, x: float): float
```

## `smoothstep`

Hermite smoothstep between `edge0` and `edge1` (WGSL `smoothstep`).

```dream
public static fun smoothstep(edge0: float, edge1: float, x: float): float
```

## `fma`

Fused multiply-add `a * b + c` (WGSL `fma`).

```dream
public static fun fma(a: float, b: float, c: float): float
```

## `length`

Euclidean length of a `GpuVec3` (WGSL `length`). Overloaded for `GpuVec2` / `GpuVec4`.

```dream
public static fun length(v: GpuVec3): float
```
