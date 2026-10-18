# GpuStageDescriptor

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuStageDescriptor`

A compiled render or ray tracing shader: `@vertex`, `@fragment`, `@ray_generation`, `@closest_hit`, `@any_hit`, `@miss`, `@intersection` or `@callable`. The shader generator creates one for every such shader as `GpuShaders.<name>`; pass it to `GpuDevice.render_pipeline` or `GpuDevice.ray_tracing_pipeline`.

```dream
public class GpuStageDescriptor
```

## `name`

The shader function's name.

```dream
public get name(): string
```

## `stage`

The stage the shader runs in.

```dream
public get stage(): GpuStage
```
