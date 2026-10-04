# Gpu

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu.md)

## `texture_sample_bias`

WGSL `textureSampleBias` — like `texture_sample` with `bias` added to the computed mip level, to sharpen or blur relative to what the derivatives chose. `@fragment` only.

```dream
public static fun texture_sample_bias( tex: GpuTexture, samp: GpuSampler, u: float, v: float, bias: float ): GpuVec4
```

## `texture_sample_grad`

WGSL `textureSampleGrad` — samples with explicit derivatives instead of the implicit ones, which is what lets a `@fragment` shader filter correctly across a discontinuous mapping.

```dream
public static fun texture_sample_grad( tex: GpuTexture, samp: GpuSampler, uv: GpuVec2, ddx: GpuVec2, ddy: GpuVec2 ): GpuVec4
```

## `texture_gather`

WGSL `textureGather` — the single component `component` (0..3) of the four texels that bilinear filtering would blend, which is how a shader hand-rolls its own filter kernel. `component` must be a literal.

```dream
public static fun texture_gather( component: int, tex: GpuTexture, samp: GpuSampler, u: float, v: float ): GpuVec4
```

## `texture_sample_compare`

WGSL `textureSampleCompare` — hardware depth comparison against `depth_ref`, returning the filtered fraction of the four neighbouring texels that pass. This is the shadow-map lookup: `tex` must be `@depth` and `samp` must be `@compare`. `@fragment` only, since it uses implicit derivatives; `texture_sample_compare_level` is the any-stage form.

```dream
public static fun texture_sample_compare( tex: GpuTexture, samp: GpuSampler, u: float, v: float, depth_ref: float ): float
```

## `texture_sample_compare_level`

WGSL `textureSampleCompareLevel` — as `texture_sample_compare`, but pinned to mip level 0 and so callable from `@compute` as well.

```dream
public static fun texture_sample_compare_level( tex: GpuTexture, samp: GpuSampler, u: float, v: float, depth_ref: float ): float
```
