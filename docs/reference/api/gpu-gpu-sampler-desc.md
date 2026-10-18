# GpuSamplerDesc

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuSamplerDesc`

The settings a new sampler is created with.  Start from `GpuSamplerDesc(GpuFilter.Linear)` or `GpuSamplerDesc(GpuFilter.Nearest)` and chain the settings you want to change.

```dream
public class GpuSamplerDesc
```

## `constructor`

Creates a sampler with the selected filter, clamp-to-edge addressing, and no comparison.

```dream
public constructor(filter: GpuFilter)
```

## `filter`

The filter used when magnifying, minifying and between mip levels.

```dream
public get filter(): GpuFilter
```

## `address_mode`

What reads outside `0.0` to `1.0` return, on every axis. Defaults to `ClampToEdge`.

```dream
public get address_mode(): GpuAddressMode
```

## `compare`

The comparison of a comparison sampler for `GpuTexture.sample_compare`, or `None` for an ordinary sampler. Defaults to `None`.

```dream
public get compare(): Option<GpuCompare>
```

## `anisotropy`

The maximum anisotropic filtering ratio, `1` (off) through `16`. Defaults to `1`.

```dream
public get anisotropy(): int
```

## `address`

Sets what reads outside `0.0` to `1.0` return, and returns this description.

```dream
public fun address(mode: GpuAddressMode): GpuSamplerDesc
```

## `comparing`

Makes this a comparison sampler using `compare`, and returns this description.

```dream
public fun comparing(compare: GpuCompare): GpuSamplerDesc
```

## `max_anisotropy`

Sets the maximum anisotropic filtering ratio, clamped to `1` through `16`, and returns this description. Anisotropy sharpens textures seen at grazing angles.

```dream
public fun max_anisotropy(ratio: int): GpuSamplerDesc
```
