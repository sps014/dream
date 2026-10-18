# GpuFeature

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuFeature`

An optional GPU capability. Ask a device with `GpuDevice.supports`; pipelines that need a capability the device lacks fail with `GpuError.UnsupportedCapability`.

```dream
public enum GpuFeature
```

## `RayTracing`

Ray tracing pipelines: `@ray_generation`, hit, miss, intersection and callable shaders.

```dream
RayTracing = 0
```

## `RayQuery`

Inline ray queries with `GpuRayQuery` in ordinary shaders.

```dream
RayQuery = 1
```

## `Bindless`

Bindless resources through `GpuHandle`.

```dream
Bindless = 2
```

## `Subgroups`

Subgroup operations such as `subgroup_add`. On WebGPU this is the optional `subgroups` feature, which the browser may not grant.

```dream
Subgroups = 3
```

## `CooperativeVectors`

Hardware cooperative vectors (`GpuCoopVec`, `GpuMlpWeights`), on Vulkan and D3D12 devices whose drivers expose them.

```dream
CooperativeVectors = 4
```

## `CooperativeMatrices`

Hardware cooperative matrices (`GpuCoopMat`): Vulkan devices with the cooperative matrix extension, and Metal on Apple-silicon GPUs (8 by 8 tiles).

```dream
CooperativeMatrices = 5
```

## `Float16`

`float16` arithmetic and `float16` buffer elements in shaders. On WebGPU this is the optional `shader-f16` feature. `GpuBuffer<float16>` uploads and downloads work without it.

```dream
Float16 = 6
```

## `Atomics64`

64-bit atomic operations on buffers.

```dream
Atomics64 = 7
```
