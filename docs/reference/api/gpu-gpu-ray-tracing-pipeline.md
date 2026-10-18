# GpuRayTracingPipeline

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuRayTracingPipeline`

Ray generation, miss, hit group and callable shaders linked into one ray tracing pipeline. Create one with `GpuDevice.ray_tracing_pipeline`; the ray generation shader's generated `GpuShaders` descriptor traces with it.

```dream
public class GpuRayTracingPipeline
```
