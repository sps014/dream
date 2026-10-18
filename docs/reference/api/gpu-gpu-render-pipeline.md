# GpuRenderPipeline

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuRenderPipeline`

A vertex and fragment shader pair plus fixed-function settings, ready to draw with. Create one with `GpuDevice.render_pipeline` and select it with `GpuRenderPass.set_pipeline` or `GpuFrame.draw`.

```dream
public class GpuRenderPipeline
```
