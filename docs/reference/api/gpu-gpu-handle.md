# GpuHandle

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuHandle`

A bindless reference to a texture or sampler that can be stored in any buffer or struct, so one shader can pick among thousands of resources at run time.  Get one from `GpuDevice.handle`. Inside a shader, `get()` returns the resource, for example `material.albedo.get().sample(material.sampler.get(), uv)`. A handle stays valid while its resource is alive. Needs `GpuFeature.Bindless`, which `GpuTarget.Dx12`, `Vulkan`, `Metal` and `Cuda` provide.

```dream
public struct GpuHandle<T>
```

## `get`

The resource this handle refers to, inside a shader.

```dream
public extern fun get(): T
```
