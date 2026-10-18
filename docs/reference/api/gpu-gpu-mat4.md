# GpuMat4

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuMat4`

A 4x4 `float` matrix stored as four columns, laid out like a GPU `float4x4`, for transforms and cameras.  `m * v` transforms the column vector `v`, and `a * b` applies `b` first, then `a`. The projection helpers use a right-handed view space where the camera looks down negative z, and map depth to `0.0` (near) through `1.0` (far), the range every `GpuBackend` expects. Use it on the CPU and in shaders alike.

```dream
public struct GpuMat4
```

## `c0`

The first column.

```dream
public c0: GpuVec4
```

## `c1`

The second column.

```dream
public c1: GpuVec4
```

## `c2`

The third column.

```dream
public c2: GpuVec4
```

## `c3`

The fourth column, which holds the translation of an affine transform.

```dream
public c3: GpuVec4
```

## `of`

Creates a matrix from its four columns.

```dream
public static fun of(c0: GpuVec4, c1: GpuVec4, c2: GpuVec4, c3: GpuVec4): GpuMat4
```

## `identity`

The identity matrix, which leaves vectors unchanged.

```dream
public static fun identity(): GpuMat4
```

## `translation`

A transform that moves points by `offset`.

```dream
public static fun translation(offset: GpuVec3): GpuMat4
```

## `scaling`

A transform that scales each axis by the matching component of `factors`.

```dream
public static fun scaling(factors: GpuVec3): GpuMat4
```

## `rotation`

A rotation by `radians` around `axis`, counter-clockwise when looking from the tip of the axis toward the origin. `axis` need not have length `1`.

```dream
public static fun rotation(axis: GpuVec3, radians: float): GpuMat4
```

## `perspective`

A perspective projection with a vertical field of view of `fov_y_radians`, a width-to-height `aspect` ratio, and clipping planes at distances `near` and `far` in front of the camera. Both distances must be positive and `near` below `far`.

```dream
public static fun perspective(
    fov_y_radians: float,
    aspect: float,
    near: float,
    far: float
): GpuMat4
```

## `look_at`

A camera transform for an eye at `eye` looking at `target`, with `up` giving the upward direction. `up` must not point along the viewing direction.

```dream
public static fun look_at(eye: GpuVec3, target: GpuVec3, up: GpuVec3): GpuMat4
```

## `row`

The row at `index` (`0` to `3`) as a vector.

```dream
public fun row(index: int): GpuVec4
```

## `transpose`

The matrix with rows and columns swapped.

```dream
public fun transpose(): GpuMat4
```
