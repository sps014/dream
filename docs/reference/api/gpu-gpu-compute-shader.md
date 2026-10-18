# GpuComputeShader

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuComputeShader`

A compiled `@compute` shader. The shader generator creates one for every compute shader as `GpuShaders.<name>`, with a typed `dispatch(device, GpuDispatch, ...)` that takes the shader's own parameters.

```dream
public class GpuComputeShader
```

## `name`

The shader function's name.

```dream
public get name(): string
```

## `workgroup_size`

The workgroup size from the shader's `@compute` attribute.

```dream
public get workgroup_size(): GpuIVec3
```
