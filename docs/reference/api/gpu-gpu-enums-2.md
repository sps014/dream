# GpuLoadOp, GpuStoreOp, GpuIndexFormat, GpuFilterMode, GpuAddressMode, GpuTextureFormat, GpuTextureDimension, GpuTextureViewDimension, GpuStorageAccess, GpuCompareFunction, GpuVertexStepMode, GpuVertexFormat, GpuPresentMode, GpuAlphaMode, GpuColorSpace, GpuPowerPreference

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-enums.md)

## `Rg16Float = 7`

```dream
Rg16Float = 7
```

## `Rgba16Float = 8`

```dream
Rgba16Float = 8
```

## `R32Float = 9`

The `r32/rg32/rgba32` floats are unfilterable without the `float32-filterable` feature; sample them with a `GpuFilterMode.Nearest` sampler unless the adapter reports it.

```dream
R32Float = 9
```

## `Rg32Float = 10`

```dream
Rg32Float = 10
```

## `Rgba32Float = 11`

```dream
Rgba32Float = 11
```

## `Rg11b10Ufloat = 12`

```dream
Rg11b10Ufloat = 12
```

## `Rgb10a2Unorm = 13`

```dream
Rgb10a2Unorm = 13
```

## `Depth16Unorm = 14`

```dream
Depth16Unorm = 14
```

## `Depth24Plus = 15`

```dream
Depth24Plus = 15
```

## `Depth24PlusStencil8 = 16`

```dream
Depth24PlusStencil8 = 16
```

## `Depth32Float = 17`

```dream
Depth32Float = 17
```

## `Depth32FloatStencil8 = 18`

Needs the `depth32float-stencil8` device feature.

```dream
Depth32FloatStencil8 = 18
```

## `Bc1RgbaUnorm = 19`

Block-compressed formats each need their family's device feature: BC on desktop, ETC2 and ASTC on mobile. Creation fails with `GpuError.unsupported` when the adapter lacks it.

```dream
Bc1RgbaUnorm = 19
```

## `Bc3RgbaUnorm = 20`

```dream
Bc3RgbaUnorm = 20
```

## `Bc5RgUnorm = 21`

```dream
Bc5RgUnorm = 21
```

## `Bc7RgbaUnorm = 22`

```dream
Bc7RgbaUnorm = 22
```

## `Etc2Rgb8Unorm = 23`

```dream
Etc2Rgb8Unorm = 23
```

## `Etc2Rgba8Unorm = 24`

```dream
Etc2Rgba8Unorm = 24
```

## `Astc4x4Unorm = 25`

```dream
Astc4x4Unorm = 25
```

## `Astc8x8Unorm = 26`

```dream
Astc8x8Unorm = 26
```

## `enum GpuTextureDimension`

Physical shape of a texture's texel grid.

```dream
public enum GpuTextureDimension
```

## `D1 = 0`

```dream
D1 = 0
```

## `D2 = 1`

```dream
D2 = 1
```

## `D3 = 2`

```dream
D3 = 2
```
