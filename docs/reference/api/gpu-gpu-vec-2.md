# GpuVec2, GpuVec3, GpuVec4, GpuMat2, GpuMat3, GpuMat4

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-vec.md)

## `mul`

Component-wise multiply (WGSL `*`).

```dream
public fun mul(other: GpuVec3): GpuVec3
```

## `div`

Component-wise divide (WGSL `/`).

```dream
public fun div(other: GpuVec3): GpuVec3
```

## `scale`

Scale by a scalar (WGSL `v * s` / `s * v`).

```dream
public fun scale(s: float): GpuVec3
```

## `div_scalar`

Divide by a scalar (WGSL `v / s`).

```dream
public fun div_scalar(s: float): GpuVec3
```

## `add_scalar`

Add a scalar to each component (WGSL `v + s`).

```dream
public fun add_scalar(s: float): GpuVec3
```

## `sub_scalar`

Subtract a scalar from each component (WGSL `v - s`).

```dream
public fun sub_scalar(s: float): GpuVec3
```

## `div_from_scalar`

Scalar divided by each component (WGSL `s / v`).

```dream
public fun div_from_scalar(s: float): GpuVec3
```

## `sub_from_scalar`

Scalar minus each component (WGSL `s - v`).

```dream
public fun sub_from_scalar(s: float): GpuVec3
```

## `neg`

Negate each component (WGSL `-v`).

```dream
public fun neg(): GpuVec3
```

## `struct GpuVec4`

Four-component float vector (`vec4<f32>` in WGSL). Used for clip-space `position`, colors (`rgba`), and fragment outputs.

```dream
public struct GpuVec4
```

## `x: float`

X / R component.

```dream
public x: float
```

## `y: float`

Y / G component.

```dream
public y: float
```

## `z: float`

Z / B component.

```dream
public z: float
```

## `w: float`

W / A component.

```dream
public w: float
```

## `of`

Builds `(x, y, z, w)` (WGSL `vec4(x, y, z, w)`).

```dream
public static fun of(x: float, y: float, z: float, w: float): GpuVec4
```

## `splat`

All components equal to `s` (WGSL `vec4(s)`).

```dream
public static fun splat(s: float): GpuVec4
```

## `add`

Component-wise add (WGSL `+`).

```dream
public fun add(other: GpuVec4): GpuVec4
```

## `sub`

Component-wise subtract (WGSL `-`).

```dream
public fun sub(other: GpuVec4): GpuVec4
```

## `mul`

Component-wise multiply (WGSL `*`).

```dream
public fun mul(other: GpuVec4): GpuVec4
```

## `div`

Component-wise divide (WGSL `/`).

```dream
public fun div(other: GpuVec4): GpuVec4
```

## `scale`

Scale by a scalar (WGSL `v * s` / `s * v`).

```dream
public fun scale(s: float): GpuVec4
```

## `div_scalar`

Divide by a scalar (WGSL `v / s`).

```dream
public fun div_scalar(s: float): GpuVec4
```

## `add_scalar`

Add a scalar to each component (WGSL `v + s`).

```dream
public fun add_scalar(s: float): GpuVec4
```

## `sub_scalar`

Subtract a scalar from each component (WGSL `v - s`).

```dream
public fun sub_scalar(s: float): GpuVec4
```
