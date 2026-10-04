# GpuSamplerDesc

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct GpuSamplerDesc`

Full sampling state, for `GpuSampler.create`. A comparison sampler (`compare` set) binds to a WGSL `sampler_comparison` and is read with `Gpu.texture_sample_compare` — the shadow-mapping path. An ordinary sampler leaves `compare` as `Option.None`; the two are not interchangeable.

```dream
public struct GpuSamplerDesc
```

## `mag_filter: GpuFilterMode`

Filtering when the texture is magnified (one texel covers many fragments).

```dream
public mag_filter: GpuFilterMode
```

## `min_filter: GpuFilterMode`

Filtering when it is minified.

```dream
public min_filter: GpuFilterMode
```

## `mip_filter: GpuFilterMode`

Filtering between mip levels.

```dream
public mip_filter: GpuFilterMode
```

## `address_u: GpuAddressMode`

```dream
public address_u: GpuAddressMode
```

## `address_v: GpuAddressMode`

```dream
public address_v: GpuAddressMode
```

## `address_w: GpuAddressMode`

```dream
public address_w: GpuAddressMode
```

## `lod_min: float`

Mip level clamp. Use these to pin sampling to a level range, e.g. a prefiltered roughness chain where the shader picks the level itself.

```dream
public lod_min: float
```

## `lod_max: float`

```dream
public lod_max: float
```

## `compare: Option<GpuCompareFunction>`

Set for a comparison (shadow) sampler: the test applied between the reference value and the fetched depth.

```dream
public compare: Option<GpuCompareFunction>
```

## `max_anisotropy: int`

Maximum anisotropic samples. `1` disables it; values above `1` require linear mag, min, and mip filtering.

```dream
public max_anisotropy: int
```

## `linear`

Trilinear, clamp-to-edge, no anisotropy — a sensible default for sampled color textures.

```dream
public static fun linear(): GpuSamplerDesc
```

## `nearest`

Point sampling, clamp-to-edge — for data textures and pixel art.

```dream
public static fun nearest(): GpuSamplerDesc
```

## `comparison`

A comparison sampler for shadow maps: linear filtering gives hardware PCF across the comparison results, and `LessEqual` passes fragments at or nearer than the stored depth.

```dream
public static fun comparison(compare: GpuCompareFunction): GpuSamplerDesc
```

## `with_address`

Sets all three address modes at once.

```dream
public fun with_address(address: GpuAddressMode): GpuSamplerDesc
```

## `with_anisotropy`

Requests `samples`-tap anisotropic filtering, forcing the linear filters it requires.

```dream
public fun with_anisotropy(samples: int): GpuSamplerDesc
```

## `with_lod`

Clamps sampling to the mip range `[min, max]`.

```dream
public fun with_lod(min: float, max: float): GpuSamplerDesc
```
