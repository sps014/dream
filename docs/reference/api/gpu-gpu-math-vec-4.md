# Gpu Math Vec

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-math-vec.md)

## `degrees`

Converts 2D vector angles from radians to degrees: `rad * (180 / pi)` (WGSL `degrees`).

```dream
public static fun degrees(rad: GpuVec2): GpuVec2
```

## `degrees`

Converts 3D Euler angles from radians to degrees: `rad * (180 / pi)` (WGSL `degrees`).

```dream
public static fun degrees(rad: GpuVec3): GpuVec3
```

## `degrees`

Converts 4D vector angles from radians to degrees: `rad * (180 / pi)` (WGSL `degrees`).

```dream
public static fun degrees(rad: GpuVec4): GpuVec4
```
