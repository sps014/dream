# GpuTextureDesc

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuTextureDesc`

The settings a new texture is created with.  Start from `GpuTextureDesc(...)` and chain the settings you want to change. Every texture can be sampled by shaders and copied to and from the CPU.

```dream
public class GpuTextureDesc
```

## `constructor`

Creates a 2D texture description with one mip, layer, and sample; width and height are texels.

```dream
public constructor(width: int, height: int, format: GpuTextureFormat)
```

## `width`

Width in texels.

```dream
public get width(): int
```

## `height`

Height in texels.

```dream
public get height(): int
```

## `format`

How each texel is stored.

```dream
public get format(): GpuTextureFormat
```

## `mip_level_count`

The number of mip levels, each half the size of the previous one. Defaults to `1`.

```dream
public get mip_level_count(): int
```

## `layer_count`

The number of array layers. Defaults to `1`.

```dream
public get layer_count(): int
```

## `sample_count`

Samples per texel for multisampled render targets: `1` or `4`. Defaults to `1`.

```dream
public get sample_count(): int
```

## `is_render_target`

Whether render passes can draw into the texture. Defaults to `false`.

```dream
public get is_render_target(): bool
```

## `is_storage`

Whether shaders can write texels directly. Defaults to `false`.

```dream
public get is_storage(): bool
```

## `mip_levels`

Sets the number of mip levels, and returns this description.

```dream
public fun mip_levels(count: int): GpuTextureDesc
```

## `layers`

Sets the number of array layers, and returns this description.

```dream
public fun layers(count: int): GpuTextureDesc
```

## `samples`

Sets the samples per texel, and returns this description.

```dream
public fun samples(count: int): GpuTextureDesc
```

## `render_target`

Sets whether render passes can draw into the texture, and returns this description. Depth formats must be render targets to be used as depth buffers.

```dream
public fun render_target(value: bool): GpuTextureDesc
```

## `storage`

Sets whether shaders can write texels directly, and returns this description.

```dream
public fun storage(value: bool): GpuTextureDesc
```
