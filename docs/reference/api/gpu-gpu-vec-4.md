# GpuVec2, GpuVec3, GpuVec4, GpuMat2, GpuMat3, GpuMat4

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-vec.md)

## `identity`

4×4 identity matrix.

```dream
public static fun identity(): GpuMat4
```

## `mul`

Matrix × vector (WGSL `*`).

```dream
public fun mul(v: GpuVec4): GpuVec4
```

## `mul_mat`

Matrix × matrix (WGSL `*`).

```dream
public fun mul_mat(other: GpuMat4): GpuMat4
```
