# RayGenerationAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `RayGenerationAttribute`

Marks a ray generation shader, the entry point of a ray tracing dispatch. It runs once per element of the dispatch grid, usually once per pixel, and starts rays with `trace_ray`. Ray tracing needs `GpuTarget.Dx12`, `Vulkan`, `Metal` or `Cuda`.

```dream
public struct RayGenerationAttribute
```
