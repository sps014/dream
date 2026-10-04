# Gpu Math Vec

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-math-vec.md)

## `ceil`

Component-wise ceil (WGSL `ceil`).

```dream
public static fun ceil(x: GpuVec2): GpuVec2
```

## `ceil`

Component-wise ceil (WGSL `ceil`).

```dream
public static fun ceil(x: GpuVec3): GpuVec3
```

## `ceil`

Component-wise ceil (WGSL `ceil`).

```dream
public static fun ceil(x: GpuVec4): GpuVec4
```

## `fract`

Component-wise fract (WGSL `fract`).

```dream
public static fun fract(x: GpuVec2): GpuVec2
```

## `fract`

Component-wise fract (WGSL `fract`).

```dream
public static fun fract(x: GpuVec3): GpuVec3
```

## `fract`

Component-wise fract (WGSL `fract`).

```dream
public static fun fract(x: GpuVec4): GpuVec4
```

## `sqrt`

Component-wise square root (WGSL `sqrt`).

```dream
public static fun sqrt(x: GpuVec2): GpuVec2
```

## `sqrt`

Component-wise square root (WGSL `sqrt`).

```dream
public static fun sqrt(x: GpuVec3): GpuVec3
```

## `sqrt`

Component-wise square root (WGSL `sqrt`).

```dream
public static fun sqrt(x: GpuVec4): GpuVec4
```

## `exp`

Component-wise `e^x` (WGSL `exp`).

```dream
public static fun exp(x: GpuVec2): GpuVec2
```

## `exp`

Component-wise `e^x` (WGSL `exp`).

```dream
public static fun exp(x: GpuVec3): GpuVec3
```

## `exp`

Component-wise `e^x` (WGSL `exp`).

```dream
public static fun exp(x: GpuVec4): GpuVec4
```

## `pow`

Component-wise `x^e` (WGSL `pow`). Scalar exponent is splat on the shader path.

```dream
public static fun pow(x: GpuVec2, e: float): GpuVec2
```

## `pow`

Component-wise `x^e` (WGSL `pow`). Scalar exponent is splat on the shader path.

```dream
public static fun pow(x: GpuVec3, e: float): GpuVec3
```

## `pow`

Component-wise `x^e` (WGSL `pow`). Scalar exponent is splat on the shader path.

```dream
public static fun pow(x: GpuVec4, e: float): GpuVec4
```

## `pow`

Component-wise `x^e` (WGSL `pow`).

```dream
public static fun pow(x: GpuVec2, e: GpuVec2): GpuVec2
```

## `pow`

Component-wise `x^e` (WGSL `pow`).

```dream
public static fun pow(x: GpuVec3, e: GpuVec3): GpuVec3
```

## `pow`

Component-wise `x^e` (WGSL `pow`).

```dream
public static fun pow(x: GpuVec4, e: GpuVec4): GpuVec4
```

## `mix`

Linear blend `a * (1 - t) + b * t` (WGSL `mix`).

```dream
public static fun mix(a: GpuVec2, b: GpuVec2, t: float): GpuVec2
```

## `mix`

Linear blend `a * (1 - t) + b * t` (WGSL `mix`).

```dream
public static fun mix(a: GpuVec3, b: GpuVec3, t: float): GpuVec3
```

## `mix`

Linear blend `a * (1 - t) + b * t` (WGSL `mix`).

```dream
public static fun mix(a: GpuVec4, b: GpuVec4, t: float): GpuVec4
```

## `mix`

Per-component mix (WGSL `mix` with a vector factor).

```dream
public static fun mix(a: GpuVec2, b: GpuVec2, t: GpuVec2): GpuVec2
```

## `mix`

Per-component mix (WGSL `mix` with a vector factor).

```dream
public static fun mix(a: GpuVec3, b: GpuVec3, t: GpuVec3): GpuVec3
```

## `mix`

Per-component mix (WGSL `mix` with a vector factor).

```dream
public static fun mix(a: GpuVec4, b: GpuVec4, t: GpuVec4): GpuVec4
```
