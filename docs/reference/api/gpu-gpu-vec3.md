# GpuVec3

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuVec3`

A vector of three `float`s, laid out like a GPU `float3`, for positions, directions, normals and RGB colors.  Use it on the CPU and in shaders alike. Arithmetic works component by component, and a `float` operand applies to every component.

```dream
public struct GpuVec3
```

## `x`

The first component.

```dream
public x: float
```

## `y`

The second component.

```dream
public y: float
```

## `z`

The third component.

```dream
public z: float
```

## `of`

Creates the vector `(x, y, z)`.

```dream
public static fun of(x: float, y: float, z: float): GpuVec3
```

## `splat`

Creates a vector with every component equal to `value`.

```dream
public static fun splat(value: float): GpuVec3
```

## `xy`

The first two components.

```dream
public get xy(): GpuVec2
```

## `dot`

The dot product.

```dream
public fun dot(other: GpuVec3): float
```

## `cross`

The cross product, perpendicular to both vectors, following the right-hand rule.

```dream
public fun cross(other: GpuVec3): GpuVec3
```

## `length`

The vector's Euclidean length.

```dream
public fun length(): float
```

## `normalize`

The vector scaled to length `1`. A zero vector gives non-finite components.

```dream
public fun normalize(): GpuVec3
```
