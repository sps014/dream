# GpuLoadOp, GpuStoreOp, GpuIndexFormat, GpuFilterMode, GpuAddressMode, GpuTextureFormat, GpuTextureDimension, GpuTextureViewDimension, GpuStorageAccess, GpuCompareFunction, GpuVertexStepMode, GpuVertexFormat, GpuPresentMode, GpuAlphaMode, GpuColorSpace, GpuPowerPreference

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-enums.md)

## `Uint32x3 = 24`

```dream
Uint32x3 = 24
```

## `Uint32x4 = 25`

```dream
Uint32x4 = 25
```

## `Sint32 = 26`

```dream
Sint32 = 26
```

## `Sint32x2 = 27`

```dream
Sint32x2 = 27
```

## `Sint32x3 = 28`

```dream
Sint32x3 = 28
```

## `Sint32x4 = 29`

```dream
Sint32x4 = 29
```

## `enum GpuPresentMode`

Swapchain presentation pacing. Native wgpu honors this; the browser canvas vsyncs with the compositor and ignores it. Unknown codes fall back to `Fifo`.

```dream
public enum GpuPresentMode
```

## `AutoVsync = 0`

```dream
AutoVsync = 0
```

## `AutoNoVsync = 1`

```dream
AutoNoVsync = 1
```

## `Fifo = 2`

Traditional vsync — the default, and the only mode guaranteed everywhere.

```dream
Fifo = 2
```

## `FifoRelaxed = 3`

```dream
FifoRelaxed = 3
```

## `Immediate = 4`

```dream
Immediate = 4
```

## `Mailbox = 5`

```dream
Mailbox = 5
```

## `enum GpuAlphaMode`

How the swapchain alpha channel composites with whatever is behind the window / canvas.

```dream
public enum GpuAlphaMode
```

## `Auto = 0`

```dream
Auto = 0
```

## `Opaque = 1`

```dream
Opaque = 1
```

## `Premultiplied = 2`

```dream
Premultiplied = 2
```

## `enum GpuColorSpace`

Canvas color space. `DisplayP3` is a browser `configure({ colorSpace })` hint; native wgpu 24 has no matching surface field, so it stays sRGB there.

```dream
public enum GpuColorSpace
```

## `Srgb = 0`

```dream
Srgb = 0
```

## `DisplayP3 = 1`

```dream
DisplayP3 = 1
```

## `enum GpuPowerPreference`

Adapter pick for `Gpu.try_init`. Only applied when a device is not already alive; after device-lost, a different preference re-requests the adapter.

```dream
public enum GpuPowerPreference
```

## `Default = 0`

Let the host pick (WebGPU omits `powerPreference`; native `PowerPreference::None`).

```dream
Default = 0
```

## `LowPower = 1`

Prefer integrated / battery-friendly GPUs.

```dream
LowPower = 1
```

## `HighPerformance = 2`

Prefer discrete GPUs — the `try_init()` default.

```dream
HighPerformance = 2
```
