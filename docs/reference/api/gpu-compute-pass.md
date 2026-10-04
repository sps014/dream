# ComputePass

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class ComputePass`

Batched compute submits: record several dispatches, then one `submit` (single queue submit).

```dream
public class ComputePass
```

## `begin`

Starts a new empty pass (no GPU work until `submit`).

```dream
public static fun begin(): ComputePass
```

## `begin_timed`

Like `begin()`, writing GPU timestamps at `begin_index` / `end_index` of `queries`. Pass `-1` for an index to skip that write. Requires `timestamp_query`.

```dream
public static fun begin_timed(queries: GpuQuerySet, begin_index: int, end_index: int): ComputePass
```

## `dispatch`

Records a named-kernel dispatch over a 3D thread grid (same semantics as `Compute.run_3d`).

```dream
public fun dispatch( kernel: string, buffers: GpuBuffer<float>[], sx: int, sy: int, sz: int ): void
```

## `dispatch_uniforms`

Like `dispatch`, with a little-endian uniform blob after extent i32s.

```dream
public fun dispatch_uniforms( kernel: string, buffers: GpuBuffer<float>[], sx: int, sy: int, sz: int, uniforms: byte[] ): void
```

## `dispatch_bind`

Records a dispatch with a typed `GpuBindList` (mixed buffers / textures / samplers).

```dream
public fun dispatch_bind( kernel: string, resources: GpuBindList, sx: int, sy: int, sz: int, uniforms: byte[] ): void
```

## `dispatch_resources`

```dream
public fun dispatch_resources( kernel: string, buffer_ids: int[], texture_ids: int[], sampler_ids: int[], sx: int, sy: int, sz: int, uniforms: byte[] ): void
```

## `dispatch_indirect`

Records an indirect dispatch (`dispatchWorkgroupsIndirect`); `args` holds 3×u32 workgroup counts.

```dream
public fun dispatch_indirect( kernel: string, buffers: GpuBuffer<float>[], args: GpuBuffer<int> ): void
```

## `dispatch_indirect_bind`

Records an indirect dispatch with a typed `GpuBindList`.

```dream
public fun dispatch_indirect_bind( kernel: string, resources: GpuBindList, args: GpuBuffer<int> ): void
```

## `write_timestamp`

Writes a GPU timestamp inside this pass. Requires `timestamp_query_inside_passes`.

```dream
public fun write_timestamp(queries: GpuQuerySet, index: int): void
```

## `submit`

Submits all recorded dispatches in one queue submit.

```dream
public async fun submit(token: Option<CancellationToken> = Option.None): Result<bool, GpuError>
```
