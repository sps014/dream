# GpuTextureFormat

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuTextureFormat`

How a texture stores each pixel.

```dream
public enum GpuTextureFormat
```

## `Rgba8Unorm`

Four 8-bit channels, read as `0.0` to `1.0`.

```dream
Rgba8Unorm = 0
```

## `Rgba8UnormSrgb`

Four 8-bit channels in the sRGB color space; reads convert to linear color.

```dream
Rgba8UnormSrgb = 1
```

## `Bgra8Unorm`

Four 8-bit channels in blue, green, red, alpha order, common for window surfaces.

```dream
Bgra8Unorm = 2
```

## `Bgra8UnormSrgb`

`Bgra8Unorm` in the sRGB color space.

```dream
Bgra8UnormSrgb = 3
```

## `R8Unorm`

One 8-bit channel, read as `0.0` to `1.0`.

```dream
R8Unorm = 4
```

## `R32Float`

One 32-bit float channel.

```dream
R32Float = 5
```

## `Rg32Float`

Two 32-bit float channels.

```dream
Rg32Float = 6
```

## `Rgba16Float`

Four 16-bit float channels.

```dream
Rgba16Float = 7
```

## `Rgba32Float`

Four 32-bit float channels.

```dream
Rgba32Float = 8
```

## `R32Uint`

One 32-bit unsigned integer channel.

```dream
R32Uint = 9
```

## `Depth32Float`

A 32-bit float depth buffer.

```dream
Depth32Float = 10
```

## `Depth24PlusStencil8`

A depth buffer of at least 24 bits plus an 8-bit stencil.

```dream
Depth24PlusStencil8 = 11
```
