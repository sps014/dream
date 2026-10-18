# GpuTargetsAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuTargetsAttribute`

Chooses which GPU APIs shaders are compiled for. This is the only place targets are chosen; there are no command-line flags for it.  On a `module` declaration it applies to every shader in the module. On a shader function it replaces the module's list for that shader. Without any `@gpu_targets`, shaders are compiled for `GpuTarget.WebGpu`, which runs in browsers and natively. Each build keeps only the listed targets it can use: wasm builds keep `WebGpu`, and native builds keep `WebGpu` plus the native APIs of the platform being built for. A shader whose list fits none of them is left out of that build with a warning, and running it returns `GpuError.BackendNotCompiled`, so one program can carry platform-specific shaders.

```dream
public struct GpuTargetsAttribute
```

## `targets`

The APIs to compile for, in no particular order.

```dream
public targets: List<GpuTarget>
```
