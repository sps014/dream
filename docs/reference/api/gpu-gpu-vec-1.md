# GpuVec2, GpuVec3, GpuVec4, GpuMat2, GpuMat3, GpuMat4

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-vec.md)

## `struct GpuVec2`

Two-component float vector (`vec2<f32>` in WGSL).

```dream
public struct GpuVec2
```

## `x: float`

X component.

```dream
public x: float
```

## `y: float`

Y component.

```dream
public y: float
```

## `of`

Builds `(x, y)` (WGSL `vec2(x, y)`).

```dream
public static fun of(x: float, y: float): GpuVec2
```

## `splat`

All components equal to `s` (WGSL `vec2(s)`).

```dream
public static fun splat(s: float): GpuVec2
```

## `add`

Component-wise add (WGSL `+`).

```dream
public fun add(other: GpuVec2): GpuVec2
```

## `sub`

Component-wise subtract (WGSL `-`).

```dream
public fun sub(other: GpuVec2): GpuVec2
```

## `mul`

Component-wise multiply (WGSL `*`).

```dream
public fun mul(other: GpuVec2): GpuVec2
```

## `div`

Component-wise divide (WGSL `/`).

```dream
public fun div(other: GpuVec2): GpuVec2
```

## `scale`

Scale by a scalar (WGSL `v * s` / `s * v`).

```dream
public fun scale(s: float): GpuVec2
```

## `div_scalar`

Divide by a scalar (WGSL `v / s`).

```dream
public fun div_scalar(s: float): GpuVec2
```

## `add_scalar`

Add a scalar to each component (WGSL `v + s`).

```dream
public fun add_scalar(s: float): GpuVec2
```

## `sub_scalar`

Subtract a scalar from each component (WGSL `v - s`).

```dream
public fun sub_scalar(s: float): GpuVec2
```

## `div_from_scalar`

Scalar divided by each component (WGSL `s / v`).

```dream
public fun div_from_scalar(s: float): GpuVec2
```

## `sub_from_scalar`

Scalar minus each component (WGSL `s - v`).

```dream
public fun sub_from_scalar(s: float): GpuVec2
```

## `neg`

Negate each component (WGSL `-v`).

```dream
public fun neg(): GpuVec2
```

## `struct GpuVec3`

Three-component float vector (`vec3<f32>` in WGSL). Common for positions and normals.

```dream
public struct GpuVec3
```

## `x: float`

X component.

```dream
public x: float
```

## `y: float`

Y component.

```dream
public y: float
```

## `z: float`

Z component.

```dream
public z: float
```

## `of`

Builds `(x, y, z)` (WGSL `vec3(x, y, z)`).

```dream
public static fun of(x: float, y: float, z: float): GpuVec3
```

## `splat`

All components equal to `s` (WGSL `vec3(s)`).

```dream
public static fun splat(s: float): GpuVec3
```

## `add`

Component-wise add (WGSL `+`).

```dream
public fun add(other: GpuVec3): GpuVec3
```

## `sub`

Component-wise subtract (WGSL `-`).

```dream
public fun sub(other: GpuVec3): GpuVec3
```
