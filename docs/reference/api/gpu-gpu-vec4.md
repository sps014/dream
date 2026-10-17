# GpuVec4

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuVec4`

A vector of four `float`s, laid out like a GPU `float4`, for RGBA colors, clip-space positions and homogeneous coordinates.  Use it on the CPU and in shaders alike. Arithmetic works component by component, and a `float` operand applies to every component.

```dream
public struct GpuVec4
```

## `x`

The first component, or red.

```dream
public x: float
```

## `y`

The second component, or green.

```dream
public y: float
```

## `z`

The third component, or blue.

```dream
public z: float
```

## `w`

The fourth component, or alpha.

```dream
public w: float
```

## `of`

Creates the vector `(x, y, z, w)`.

```dream
public static fun of(x: float, y: float, z: float, w: float): GpuVec4
```

## `splat`

Creates a vector with every component equal to `value`.

```dream
public static fun splat(value: float): GpuVec4
```

## `xy`

The first two components.

```dream
public get xy(): GpuVec2
```

## `xyz`

The first three components.

```dream
public get xyz(): GpuVec3
```

## `dot`

The dot product.

```dream
public fun dot(other: GpuVec4): float
```

## `length`

The vector's Euclidean length.

```dream
public fun length(): float
```

## `normalize`

The vector scaled to length `1`. A zero vector gives non-finite components.

```dream
public fun normalize(): GpuVec4
```
