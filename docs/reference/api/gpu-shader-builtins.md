# Shader built-ins

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `global_id`

This thread's position in the whole compute dispatch: `workgroup_id() * workgroup size + local_id()`. With `GpuDispatch.threads_1d(n)`, `global_id().x` runs from `0` to at least `n - 1`.

```dream
public extern fun global_id(): GpuIVec3
```

## `local_id`

This thread's position within its workgroup, from `0` to the `@compute` size minus one.

```dream
public extern fun local_id(): GpuIVec3
```

## `local_index`

`local_id()` flattened to one number: `x + y * size.x + z * size.x * size.y`.

```dream
public extern fun local_index(): int
```

## `workgroup_id`

This thread's workgroup's position in the dispatch.

```dream
public extern fun workgroup_id(): GpuIVec3
```

## `workgroup_count`

The number of workgroups in the dispatch.

```dream
public extern fun workgroup_count(): GpuIVec3
```

## `workgroup_barrier`

Waits until every thread of the workgroup reaches this call, and makes their earlier writes to workgroup memory visible. Every thread must reach the same call.

```dream
public extern fun workgroup_barrier(): void
```

## `storage_barrier`

Like `workgroup_barrier`, and also makes the workgroup's earlier buffer writes visible to each other.

```dream
public extern fun storage_barrier(): void
```

## `workgroup_array`

Declares `length` elements of workgroup memory, shared by every thread of the workgroup and uninitialized at the start. `length` must be a constant. Call it once per array at the top of the shader.

```dream
public extern fun workgroup_array<T>(length: int): GpuWorkgroupArray<T>
```

## `vertex_id`

The index of the vertex being processed: `0` to the draw's vertex count minus one, or the index buffer's value for indexed draws.

```dream
public extern fun vertex_id(): int
```

## `instance_id`

The index of the instance being drawn, from `0`.

```dream
public extern fun instance_id(): int
```

## `discard_fragment`

Discards the current pixel: nothing is written for it. The shader stops at this call.

```dream
public extern fun discard_fragment(): void
```
