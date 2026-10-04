# Gpu Math Vec

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-math-vec.md)

## `min`

Component-wise min (WGSL `min`).

```dream
public static fun min(a: GpuVec2, b: GpuVec2): GpuVec2
```

## `min`

Component-wise min (WGSL `min`).

```dream
public static fun min(a: GpuVec3, b: GpuVec3): GpuVec3
```

## `min`

Component-wise min (WGSL `min`).

```dream
public static fun min(a: GpuVec4, b: GpuVec4): GpuVec4
```

## `max`

Component-wise max (WGSL `max`).

```dream
public static fun max(a: GpuVec2, b: GpuVec2): GpuVec2
```

## `max`

Component-wise max (WGSL `max`).

```dream
public static fun max(a: GpuVec3, b: GpuVec3): GpuVec3
```

## `max`

Component-wise max (WGSL `max`).

```dream
public static fun max(a: GpuVec4, b: GpuVec4): GpuVec4
```

## `abs`

Component-wise abs (WGSL `abs`).

```dream
public static fun abs(a: GpuVec2): GpuVec2
```

## `abs`

Component-wise abs (WGSL `abs`).

```dream
public static fun abs(a: GpuVec3): GpuVec3
```

## `abs`

Component-wise abs (WGSL `abs`).

```dream
public static fun abs(a: GpuVec4): GpuVec4
```

## `clamp`

Component-wise clamp into `[lo, hi]` (WGSL `clamp`).

```dream
public static fun clamp(x: GpuVec2, lo: float, hi: float): GpuVec2
```

## `clamp`

Component-wise clamp into `[lo, hi]` (WGSL `clamp`).

```dream
public static fun clamp(x: GpuVec3, lo: float, hi: float): GpuVec3
```

## `clamp`

Component-wise clamp into `[lo, hi]` (WGSL `clamp`).

```dream
public static fun clamp(x: GpuVec4, lo: float, hi: float): GpuVec4
```

## `clamp`

Per-component clamp (WGSL `clamp` with vector bounds).

```dream
public static fun clamp(x: GpuVec2, lo: GpuVec2, hi: GpuVec2): GpuVec2
```

## `clamp`

Per-component clamp (WGSL `clamp` with vector bounds).

```dream
public static fun clamp(x: GpuVec3, lo: GpuVec3, hi: GpuVec3): GpuVec3
```

## `clamp`

Per-component clamp (WGSL `clamp` with vector bounds).

```dream
public static fun clamp(x: GpuVec4, lo: GpuVec4, hi: GpuVec4): GpuVec4
```

## `saturate`

Clamps each component into `[0, 1]` (WGSL `saturate` / `clamp(x, 0, 1)`).

```dream
public static fun saturate(x: GpuVec2): GpuVec2
```

## `saturate`

Clamps each component into `[0, 1]` (WGSL `saturate` / `clamp(x, 0, 1)`).

```dream
public static fun saturate(x: GpuVec3): GpuVec3
```

## `saturate`

Clamps each component into `[0, 1]` (WGSL `saturate` / `clamp(x, 0, 1)`).

```dream
public static fun saturate(x: GpuVec4): GpuVec4
```

## `sign`

Component-wise sign: −1, 0, or 1 (WGSL `sign`).

```dream
public static fun sign(x: GpuVec2): GpuVec2
```

## `sign`

Component-wise sign: −1, 0, or 1 (WGSL `sign`).

```dream
public static fun sign(x: GpuVec3): GpuVec3
```

## `sign`

Component-wise sign: −1, 0, or 1 (WGSL `sign`).

```dream
public static fun sign(x: GpuVec4): GpuVec4
```

## `floor`

Component-wise floor (WGSL `floor`).

```dream
public static fun floor(x: GpuVec2): GpuVec2
```

## `floor`

Component-wise floor (WGSL `floor`).

```dream
public static fun floor(x: GpuVec3): GpuVec3
```

## `floor`

Component-wise floor (WGSL `floor`).

```dream
public static fun floor(x: GpuVec4): GpuVec4
```
