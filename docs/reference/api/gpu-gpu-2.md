# Gpu

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu.md)

## `atomic_compare_exchange`

WGSL `atomicCompareExchangeWeak`: stores `v` into `buf[i]` only if it currently holds `cmp`. Returns the old value either way, so the store happened exactly when the result equals `cmp`. "Weak" means it may also fail spuriously, hence the retry loop:  let old = Gpu.atomic_compare_exchange(buf, i, seen, next); if old == seen { break; }   // won; otherwise re-read and try again

```dream
public static fun atomic_compare_exchange( buf: GpuBuffer<int>, i: int, cmp: int, v: int ): int
```

## `dpdx`

Screen-space partial derivative with respect to window X coordinate (WGSL `dpdx`); `@fragment` stage only.

```dream
public static fun dpdx(v: float): float
```

## `dpdx`

Component-wise screen-space X derivative of a 2D vector (`dpdx(v.x)`, `dpdx(v.y)`); `@fragment` stage only.

```dream
public static fun dpdx(v: GpuVec2): GpuVec2
```

## `dpdx`

Component-wise screen-space X derivative of a 3D vector (e.g. world position); `@fragment` stage only.

```dream
public static fun dpdx(v: GpuVec3): GpuVec3
```

## `dpdx`

Component-wise screen-space X derivative of a 4D vector; `@fragment` stage only.

```dream
public static fun dpdx(v: GpuVec4): GpuVec4
```

## `dpdy`

Screen-space partial derivative with respect to window Y coordinate (WGSL `dpdy`); `@fragment` stage only.

```dream
public static fun dpdy(v: float): float
```

## `dpdy`

Component-wise screen-space Y derivative of a 2D vector (`dpdy(v.x)`, `dpdy(v.y)`); `@fragment` stage only.

```dream
public static fun dpdy(v: GpuVec2): GpuVec2
```

## `dpdy`

Component-wise screen-space Y derivative of a 3D vector (e.g. world position); `@fragment` stage only.

```dream
public static fun dpdy(v: GpuVec3): GpuVec3
```

## `dpdy`

Component-wise screen-space Y derivative of a 4D vector; `@fragment` stage only.

```dream
public static fun dpdy(v: GpuVec4): GpuVec4
```

## `fwidth`

Screen-space derivative magnitude `abs(dpdx(v)) + abs(dpdy(v))` (WGSL `fwidth`) for procedural antialiasing; `@fragment` only.

```dream
public static fun fwidth(v: float): float
```

## `fwidth`

Component-wise derivative magnitude for 2D coordinates (e.g. UV gradients, grid lines); `@fragment` only.

```dream
public static fun fwidth(v: GpuVec2): GpuVec2
```

## `fwidth`

Component-wise derivative magnitude for 3D coordinates (e.g. world-space normal estimation); `@fragment` only.

```dream
public static fun fwidth(v: GpuVec3): GpuVec3
```

## `fwidth`

Component-wise derivative magnitude for 4D coordinates; `@fragment` only.

```dream
public static fun fwidth(v: GpuVec4): GpuVec4
```

## `texture_dimensions`

Returns the texture's `(width, height)` in texels as a float vector (WGSL `textureDimensions`).

```dream
public static fun texture_dimensions(tex: GpuTexture): GpuVec2
```

## `texture_load`

WGSL `textureLoad` (texel fetch, no filtering) for `@compute`. Reads mip level 0; use `texture_load_level` for a specific level.

```dream
public static fun texture_load(tex: GpuTexture, x: int, y: int): GpuVec4
```

## `texture_load_level`

WGSL `textureLoad` at an explicit mip `level`.

```dream
public static fun texture_load_level( tex: GpuTexture, x: int, y: int, level: int ): GpuVec4
```

## `texture_load_layer`

WGSL `textureLoad` from one `layer` of a `@view("2d-array")` texture.

```dream
public static fun texture_load_layer( tex: GpuTexture, x: int, y: int, layer: int, level: int ): GpuVec4
```

## `texture_num_levels`

Mip level count of `tex` (WGSL `textureNumLevels`).

```dream
public static fun texture_num_levels(tex: GpuTexture): int
```

## `texture_num_layers`

Array layer count of `tex` (WGSL `textureNumLayers`).

```dream
public static fun texture_num_layers(tex: GpuTexture): int
```

## `texture_store`

WGSL `textureStore` for `@compute`.

```dream
public static fun texture_store( tex: GpuTexture, x: int, y: int, r: float, g: float, b: float, a: float ): void
```

## `texture_sample_level`

WGSL `textureSampleLevel` — filtered sample at an explicit mip `level`, so it is usable from `@compute` where there are no implicit derivatives.

```dream
public static fun texture_sample_level( tex: GpuTexture, samp: GpuSampler, u: float, v: float, level: float ): GpuVec4
```

## `texture_sample`

WGSL `textureSample` for `@fragment`.

```dream
public static fun texture_sample( tex: GpuTexture, samp: GpuSampler, u: float, v: float ): GpuVec4
```

## `texture_sample_cube`

Samples a cubemap texture along the 3D direction vector `dir` (WGSL `textureSample`).

```dream
public static fun texture_sample_cube( tex: GpuTexture, samp: GpuSampler, dir: GpuVec3 ): GpuVec4
```

## `texture_sample_layer`

Samples one `layer` of a `@view("2d-array")` texture (WGSL `textureSample`); `@fragment` only.

```dream
public static fun texture_sample_layer( tex: GpuTexture, samp: GpuSampler, u: float, v: float, layer: int ): GpuVec4
```
