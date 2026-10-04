# GpuLoadOp, GpuStoreOp, GpuIndexFormat, GpuFilterMode, GpuAddressMode, GpuTextureFormat, GpuTextureDimension, GpuTextureViewDimension, GpuStorageAccess, GpuCompareFunction, GpuVertexStepMode, GpuVertexFormat, GpuPresentMode, GpuAlphaMode, GpuColorSpace, GpuPowerPreference

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-enums.md)

## `enum GpuLoadOp`

Color attachment load operation for draw helpers (matches host `load_op` ints).

```dream
public enum GpuLoadOp
```

## `Clear = 0`

Clear the attachment to the draw clear color.

```dream
Clear = 0
```

## `Load = 1`

Keep the previous contents of the attachment.

```dream
Load = 1
```

## `enum GpuStoreOp`

Attachment store operation for recorded render passes.

```dream
public enum GpuStoreOp
```

## `Store = 0`

Keep the rendered contents.

```dream
Store = 0
```

## `Discard = 1`

Throw the contents away (cheaper for attachments nothing reads back).

```dream
Discard = 1
```

## `enum GpuIndexFormat`

Index buffer element width for `GpuRenderPassEncoder.set_index_buffer`.

```dream
public enum GpuIndexFormat
```

## `Uint32 = 0`

32-bit indices (`GpuBuffer<int>`).

```dream
Uint32 = 0
```

## `Uint16 = 1`

16-bit indices, two per `int` slot.

```dream
Uint16 = 1
```

## `enum GpuFilterMode`

Mag/min filter for `GpuSampler.create` (matches host filter ints).

```dream
public enum GpuFilterMode
```

## `Nearest = 0`

Nearest-neighbor sampling.

```dream
Nearest = 0
```

## `Linear = 1`

Linear interpolation between texels.

```dream
Linear = 1
```

## `enum GpuAddressMode`

Address mode for `GpuSampler.create` (matches host address ints).

```dream
public enum GpuAddressMode
```

## `ClampToEdge = 0`

Clamp UVs to the edge texel.

```dream
ClampToEdge = 0
```

## `Repeat = 1`

Repeat the texture.

```dream
Repeat = 1
```

## `MirrorRepeat = 2`

Mirror on every other repeat.

```dream
MirrorRepeat = 2
```

## `enum GpuTextureFormat`

Texel format of a `GpuTexture`.

```dream
public enum GpuTextureFormat
```

## `R8Unorm = 0`

```dream
R8Unorm = 0
```

## `Rg8Unorm = 1`

```dream
Rg8Unorm = 1
```

## `Rgba8Unorm = 2`

```dream
Rgba8Unorm = 2
```

## `Rgba8UnormSrgb = 3`

```dream
Rgba8UnormSrgb = 3
```

## `Bgra8Unorm = 4`

```dream
Bgra8Unorm = 4
```

## `Bgra8UnormSrgb = 5`

```dream
Bgra8UnormSrgb = 5
```

## `R16Float = 6`

```dream
R16Float = 6
```
