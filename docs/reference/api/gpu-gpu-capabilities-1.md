# GpuCapabilities

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-capabilities.md)

## `struct GpuCapabilities`

The optional features and raised limits the GPU device was actually created with. WebGPU only lets a shader or resource touch a feature the *device* opted into at creation time; reaching for an un-requested one is device-loss-grade rather than a recoverable validation error. These flags are therefore the authoritative answer to "may I use this?", not merely "does the hardware have it?" — gate on them rather than probing and recovering.

```dream
public struct GpuCapabilities
```

## `shader_float16: bool`

Half-precision (`f16`) arithmetic and storage in shaders.

```dream
public shader_float16: bool
```

## `subgroup: bool`

```dream
public subgroup: bool
```

## `subgroup_barrier: bool`

`subgroupBarrier`. Native-only today: WebGPU exposes no counterpart.

```dream
public subgroup_barrier: bool
```

## `tile_float16: bool`

Cooperative ("subgroup") matrix tiles accumulating in `float16`.

```dream
public tile_float16: bool
```

## `tile_float: bool`

Cooperative matrix tiles accumulating in `float`.

```dream
public tile_float: bool
```

## `tile_n: int`

Square cooperative-matrix tile width, `8` or `16`; `0` when tiles are unavailable.

```dream
public tile_n: int
```

## `texture_compression_bc: bool`

BC ("DXT"/S3TC) texture formats — desktop GPUs. `GpuTexture.create` with a `Bc*` format fails with `GpuError.unsupported` unless this is set.

```dream
public texture_compression_bc: bool
```

## `texture_compression_etc2: bool`

ETC2 texture formats — mobile GPUs.

```dream
public texture_compression_etc2: bool
```

## `texture_compression_astc: bool`

ASTC texture formats — mobile GPUs.

```dream
public texture_compression_astc: bool
```

## `depth32_float_stencil8: bool`

The combined `depth32float-stencil8` format.

```dream
public depth32_float_stencil8: bool
```

## `float32_filterable: bool`

Linear filtering of 32-bit float textures.

```dream
public float32_filterable: bool
```

## `timestamp_query: bool`

GPU timestamp queries (`timestampWrites` / `resolveQuerySet`).

```dream
public timestamp_query: bool
```

## `timestamp_query_inside_encoders: bool`

`writeTimestamp` on a command encoder between passes (native wgpu / some browsers).

```dream
public timestamp_query_inside_encoders: bool
```

## `timestamp_query_inside_passes: bool`

`writeTimestamp` inside an open compute or render pass.

```dream
public timestamp_query_inside_passes: bool
```

## `max_buffer_bytes: long`

Largest single buffer allocation.

```dream
public max_buffer_bytes: long
```

## `max_storage_binding_bytes: long`

Largest range bindable to one storage-buffer slot.

```dream
public max_storage_binding_bytes: long
```

## `max_workgroup_storage_bytes: int`

`var<workgroup>` bytes available to one compute workgroup.

```dream
public max_workgroup_storage_bytes: int
```

## `max_invocations_per_workgroup: int`

Invocations in one workgroup, i.e. the product of the three `@workgroup_size` dimensions.

```dream
public max_invocations_per_workgroup: int
```

## `max_workgroup_size_x: int`

Per-dimension ceiling on `@workgroup_size`.

```dream
public max_workgroup_size_x: int
```

## `max_workgroup_size_y: int`

```dream
public max_workgroup_size_y: int
```

## `max_workgroup_size_z: int`

```dream
public max_workgroup_size_z: int
```

## `max_workgroups_per_dimension: int`

Per-dimension ceiling on the workgroup count of a single dispatch.

```dream
public max_workgroups_per_dimension: int
```

## `min_subgroup_size: int`

Subgroup width range; both `0` unless `subgroup` is set.

```dream
public min_subgroup_size: int
```
