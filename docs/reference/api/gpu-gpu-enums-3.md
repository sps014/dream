# GpuLoadOp, GpuStoreOp, GpuIndexFormat, GpuFilterMode, GpuAddressMode, GpuTextureFormat, GpuTextureDimension, GpuTextureViewDimension, GpuStorageAccess, GpuCompareFunction, GpuVertexStepMode, GpuVertexFormat, GpuPresentMode, GpuAlphaMode, GpuColorSpace, GpuPowerPreference

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-enums.md)

## `enum GpuTextureViewDimension`

How a binding interprets a texture's layers. Must match the WGSL `texture_*` type the shader declares: `texture_2d_array` needs `D2Array`, `texture_cube` needs `Cube`, and so on.

```dream
public enum GpuTextureViewDimension
```

## `D1 = 0`

```dream
D1 = 0
```

## `D2 = 1`

```dream
D2 = 1
```

## `D2Array = 2`

```dream
D2Array = 2
```

## `Cube = 3`

```dream
Cube = 3
```

## `CubeArray = 4`

```dream
CubeArray = 4
```

## `D3 = 5`

```dream
D3 = 5
```

## `enum GpuStorageAccess`

Access a shader has to a `texture_storage_*` binding.

```dream
public enum GpuStorageAccess
```

## `ReadOnly = 0`

```dream
ReadOnly = 0
```

## `WriteOnly = 1`

```dream
WriteOnly = 1
```

## `ReadWrite = 2`

Needs a storage format that supports read-write access (`r32float`, `rgba8unorm`, …).

```dream
ReadWrite = 2
```

## `enum GpuCompareFunction`

Depth/stencil comparison, used by comparison samplers (shadow maps) and depth state.

```dream
public enum GpuCompareFunction
```

## `Never = 0`

```dream
Never = 0
```

## `Less = 1`

```dream
Less = 1
```

## `Equal = 2`

```dream
Equal = 2
```

## `LessEqual = 3`

```dream
LessEqual = 3
```

## `Greater = 4`

```dream
Greater = 4
```

## `NotEqual = 5`

```dream
NotEqual = 5
```

## `GreaterEqual = 6`

```dream
GreaterEqual = 6
```

## `Always = 7`

```dream
Always = 7
```

## `enum GpuVertexStepMode`

Whether a vertex buffer advances per vertex or per instance.

```dream
public enum GpuVertexStepMode
```

## `Vertex = 0`

```dream
Vertex = 0
```

## `Instance = 1`

```dream
Instance = 1
```

## `enum GpuVertexFormat`

Element format of a single vertex attribute. Discriminants are host wire values and are append-only, mirrored by `GpuVertexFormat` handling in both hosts.

```dream
public enum GpuVertexFormat
```
