# GpuSurface

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuSurface`

The part of a window a `GpuDevice` draws frames into.  Create one with `GpuDevice.create_surface`. Each loop iteration, call `begin_frame`, draw, then `GpuFrame.present`. When the window reports `WindowEvent.Resized`, pass the new size to `resize` before the next frame.

```dream
public class GpuSurface
```

## `color_format`

The format of frame targets; pass it to `GpuRenderPipelineDesc.defaults`.

```dream
public get color_format(): GpuTextureFormat
```

## `width`

Frame width in physical pixels.

```dream
public get width(): int
```

## `height`

Frame height in physical pixels.

```dream
public get height(): int
```

## `resize`

Resizes later frames to `width` by `height` physical pixels, the size carried by `WindowEvent.Resized`. Non-positive sizes, such as a minimized window's, are ignored.

```dream
public fun resize(width: int, height: int): Result<bool, GpuError>
```

## `begin_frame`

Waits for the next frame target and starts a frame that clears it to `clear`. With vsync on, this is where the loop waits for the display.

```dream
public async fun begin_frame(clear: GpuVec4): Result<GpuFrame, GpuError>
```
