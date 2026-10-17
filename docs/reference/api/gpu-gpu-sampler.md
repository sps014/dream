# GpuSampler

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuSampler`

Filtering and wrapping settings that shaders pass to `GpuTexture.sample`. Create one with `GpuDevice.sampler`.

```dream
public class GpuSampler
```
