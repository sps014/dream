# GpuStageShader

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuStageShader`

A render or ray tracing shader as the shader generator exposes it: `GpuShaders.<name>` for every `@vertex`, `@fragment`, `@ray_generation`, `@closest_hit`, `@any_hit`, `@miss`, `@intersection` and `@callable` shader. Pass it to `GpuDevice.render_pipeline`, `GpuDevice.ray_tracing_pipeline` or `GpuHitGroup`. The generated type also has one `GpuBindingSlot` property per resource parameter and, for ray generation shaders, a typed `trace` method.

```dream
public interface GpuStageShader
```

## `descriptor`

The compiled shader: its entry point name, stage and stage interface.

```dream
fun descriptor(): GpuStageDescriptor
```
