# GpuVec2, GpuVec3, GpuVec4, GpuMat2, GpuMat3, GpuMat4

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-vec.md)

## `div_from_scalar`

Scalar divided by each component (WGSL `s / v`).

```dream
public fun div_from_scalar(s: float): GpuVec4
```

## `sub_from_scalar`

Scalar minus each component (WGSL `s - v`).

```dream
public fun sub_from_scalar(s: float): GpuVec4
```

## `neg`

Negate each component (WGSL `-v`).

```dream
public fun neg(): GpuVec4
```

## `struct GpuMat2`

2×2 column-major float matrix (`mat2x2<f32>` in WGSL).

```dream
public struct GpuMat2
```

## `c0: GpuVec2`

First column.

```dream
public c0: GpuVec2
```

## `c1: GpuVec2`

Second column.

```dream
public c1: GpuVec2
```

## `of`

Builds a matrix from column vectors (WGSL `mat2x2(c0, c1)`).

```dream
public static fun of(c0: GpuVec2, c1: GpuVec2): GpuMat2
```

## `identity`

2×2 identity matrix (WGSL `mat2x2(1, 0, 0, 1)`).

```dream
public static fun identity(): GpuMat2
```

## `mul`

Matrix × vector (WGSL `*`).

```dream
public fun mul(v: GpuVec2): GpuVec2
```

## `mul_mat`

Matrix × matrix (WGSL `*`).

```dream
public fun mul_mat(other: GpuMat2): GpuMat2
```

## `struct GpuMat3`

3×3 column-major float matrix (`mat3x3<f32>` in WGSL).

```dream
public struct GpuMat3
```

## `c0: GpuVec3`

First column.

```dream
public c0: GpuVec3
```

## `c1: GpuVec3`

Second column.

```dream
public c1: GpuVec3
```

## `c2: GpuVec3`

Third column.

```dream
public c2: GpuVec3
```

## `of`

Builds a matrix from column vectors (WGSL `mat3x3(c0, c1, c2)`).

```dream
public static fun of(c0: GpuVec3, c1: GpuVec3, c2: GpuVec3): GpuMat3
```

## `identity`

3×3 identity matrix.

```dream
public static fun identity(): GpuMat3
```

## `mul`

Matrix × vector (WGSL `*`).

```dream
public fun mul(v: GpuVec3): GpuVec3
```

## `mul_mat`

Matrix × matrix (WGSL `*`).

```dream
public fun mul_mat(other: GpuMat3): GpuMat3
```

## `struct GpuMat4`

4×4 column-major float matrix (`mat4x4<f32>` in WGSL).

```dream
public struct GpuMat4
```

## `c0: GpuVec4`

First column.

```dream
public c0: GpuVec4
```

## `c1: GpuVec4`

Second column.

```dream
public c1: GpuVec4
```

## `c2: GpuVec4`

Third column.

```dream
public c2: GpuVec4
```

## `c3: GpuVec4`

Fourth column.

```dream
public c3: GpuVec4
```

## `of`

Builds a matrix from column vectors (WGSL `mat4x4(c0, c1, c2, c3)`).

```dream
public static fun of(c0: GpuVec4, c1: GpuVec4, c2: GpuVec4, c3: GpuVec4): GpuMat4
```
