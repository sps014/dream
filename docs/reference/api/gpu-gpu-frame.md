# GpuFrame

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuFrame`

One frame of a window surface, from `GpuSurface.begin_frame` until `present`.  Draw with `draw` for simple cases, or record into `pass` for vertex buffers, textures and other settings. For extra passes, such as post-processing, use `encoder`; every pass runs in the order recorded, and the first frame pass clears `target` to the color given to `begin_frame`. Nothing appears on screen until `present`.

```dream
public class GpuFrame
```

## `target`

The texture shown when the frame is presented. Valid until `present`.

```dream
public get target(): GpuTexture
```

## `pass`

The render pass that draws into `target`, begun on first use. Draws recorded into it appear in order with everything else in the frame.

```dream
public get pass(): GpuRenderPass
```

## `encoder`

The encoder that records this frame's work, for passes of your own. The frame's `pass` is ended first, so your passes run after the draws recorded so far.

```dream
public get encoder(): GpuCommandEncoder
```

## `draw`

Draws `vertex_count` vertices with `pipeline` into `target`, for shaders that need no vertex buffers or bound resources.

```dream
public fun draw(pipeline: GpuRenderPipeline, vertex_count: int): Result<bool, GpuError>
```

## `present`

Submits everything recorded for the frame and shows `target` in the window.

```dream
public async fun present(): Result<bool, GpuError>
```
