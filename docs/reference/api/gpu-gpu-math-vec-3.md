# Gpu Math Vec

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-math-vec.md)

## `length`

Euclidean length of a `GpuVec2` (WGSL `length`).

```dream
public static fun length(v: GpuVec2): float
```

## `length`

Euclidean length of a `GpuVec4` (WGSL `length`).

```dream
public static fun length(v: GpuVec4): float
```

## `normalize`

Unit `GpuVec2`; near-zero input yields `(1, 0)` on CPU (WGSL `normalize`).

```dream
public static fun normalize(v: GpuVec2): GpuVec2
```

## `normalize`

Unit `GpuVec4`; near-zero input yields `(0, 0, 0, 1)` on CPU (WGSL `normalize`).

```dream
public static fun normalize(v: GpuVec4): GpuVec4
```

## `dot`

Dot product of two `GpuVec2`s (WGSL `dot`).

```dream
public static fun dot(a: GpuVec2, b: GpuVec2): float
```

## `dot`

Dot product of two `GpuVec4`s (WGSL `dot`).

```dream
public static fun dot(a: GpuVec4, b: GpuVec4): float
```

## `distance`

Euclidean distance between two 2D points: `length(a - b)` (WGSL `distance`).

```dream
public static fun distance(a: GpuVec2, b: GpuVec2): float
```

## `distance`

Euclidean distance between two 3D points in space: `length(a - b)` (WGSL `distance`).

```dream
public static fun distance(a: GpuVec3, b: GpuVec3): float
```

## `distance`

Euclidean distance between two 4D vectors/homogeneous points: `length(a - b)` (WGSL `distance`).

```dream
public static fun distance(a: GpuVec4, b: GpuVec4): float
```

## `exp2`

Component-wise base-2 exponential: `(2^x, 2^y)` (WGSL `exp2`).

```dream
public static fun exp2(x: GpuVec2): GpuVec2
```

## `exp2`

Component-wise base-2 exponential: `(2^x, 2^y, 2^z)` (WGSL `exp2`).

```dream
public static fun exp2(x: GpuVec3): GpuVec3
```

## `exp2`

Component-wise base-2 exponential: `(2^x, 2^y, 2^z, 2^w)` (WGSL `exp2`).

```dream
public static fun exp2(x: GpuVec4): GpuVec4
```

## `log2`

Component-wise base-2 logarithm: `(log2(x), log2(y))` (WGSL `log2`).

```dream
public static fun log2(x: GpuVec2): GpuVec2
```

## `log2`

Component-wise base-2 logarithm: `(log2(x), log2(y), log2(z))` (WGSL `log2`).

```dream
public static fun log2(x: GpuVec3): GpuVec3
```

## `log2`

Component-wise base-2 logarithm: `(log2(x), log2(y), log2(z), log2(w))` (WGSL `log2`).

```dream
public static fun log2(x: GpuVec4): GpuVec4
```

## `round`

Rounds each 2D vector component to the nearest integer, ties to even (WGSL `round`).

```dream
public static fun round(x: GpuVec2): GpuVec2
```

## `round`

Rounds each 3D vector component to the nearest integer, ties to even (WGSL `round`).

```dream
public static fun round(x: GpuVec3): GpuVec3
```

## `round`

Rounds each 4D vector component to the nearest integer, ties to even (WGSL `round`).

```dream
public static fun round(x: GpuVec4): GpuVec4
```

## `trunc`

Truncates fractional parts toward zero for each 2D vector component (WGSL `trunc`).

```dream
public static fun trunc(x: GpuVec2): GpuVec2
```

## `trunc`

Truncates fractional parts toward zero for each 3D vector component (WGSL `trunc`).

```dream
public static fun trunc(x: GpuVec3): GpuVec3
```

## `trunc`

Truncates fractional parts toward zero for each 4D vector component (WGSL `trunc`).

```dream
public static fun trunc(x: GpuVec4): GpuVec4
```

## `radians`

Converts 2D vector angles from degrees to radians: `deg * (pi / 180)` (WGSL `radians`).

```dream
public static fun radians(deg: GpuVec2): GpuVec2
```

## `radians`

Converts 3D Euler angles from degrees to radians: `deg * (pi / 180)` (WGSL `radians`).

```dream
public static fun radians(deg: GpuVec3): GpuVec3
```

## `radians`

Converts 4D vector angles from degrees to radians: `deg * (pi / 180)` (WGSL `radians`).

```dream
public static fun radians(deg: GpuVec4): GpuVec4
```
