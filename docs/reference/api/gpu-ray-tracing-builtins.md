# Ray tracing built-ins

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `trace_ray`

Traces `ray` against `scene` and waits for the hit or miss shader to fill in `payload`. Hits run the first hit group plus the instance's hit group offset; misses run the first miss shader. `mask` is compared with each instance's mask, so `255` sees everything.

```dream
public extern fun trace_ray<P>(
    scene: GpuAccelerationStructure,
    flags: GpuRayFlags,
    mask: uint,
    ray: GpuRay,
    ref payload: P
): void
```

## `trace_ray_indexed`

Like `trace_ray`, selecting hit groups as `hit_group_offset + instance offset * hit_group_stride` and the miss shader by `miss_index`, for several ray types in one pipeline.

```dream
public extern fun trace_ray_indexed<P>(
    scene: GpuAccelerationStructure,
    flags: GpuRayFlags,
    mask: uint,
    hit_group_offset: int,
    hit_group_stride: int,
    miss_index: int,
    ray: GpuRay,
    ref payload: P
): void
```

## `call_shader`

Runs the pipeline's callable shader number `index` with `data`.

```dream
public extern fun call_shader<D>(index: int, ref data: D): void
```

## `dispatch_ray_index`

This thread's position in the ray tracing dispatch grid, for example its pixel.

```dream
public extern fun dispatch_ray_index(): GpuIVec3
```

## `dispatch_ray_dimensions`

The size of the ray tracing dispatch grid.

```dream
public extern fun dispatch_ray_dimensions(): GpuIVec3
```

## `world_ray_origin`

The current ray's origin in world space.

```dream
public extern fun world_ray_origin(): GpuVec3
```

## `world_ray_direction`

The current ray's direction in world space.

```dream
public extern fun world_ray_direction(): GpuVec3
```

## `object_ray_origin`

The current ray's origin in the hit instance's object space.

```dream
public extern fun object_ray_origin(): GpuVec3
```

## `object_ray_direction`

The current ray's direction in the hit instance's object space.

```dream
public extern fun object_ray_direction(): GpuVec3
```

## `ray_t_min`

The current ray's `t_min`.

```dream
public extern fun ray_t_min(): float
```

## `ray_t_current`

The distance to the current hit, or the ray's `t_max` in miss shaders.

```dream
public extern fun ray_t_current(): float
```

## `hit_instance_id`

The `GpuInstance.with_id` value of the hit instance.

```dream
public extern fun hit_instance_id(): int
```

## `hit_primitive_index`

The index of the hit triangle or box within its geometry.

```dream
public extern fun hit_primitive_index(): int
```

## `hit_kind`

The value the intersection shader passed to `report_hit` as `kind`, or `254` and `255` for front- and back-facing triangles.

```dream
public extern fun hit_kind(): int
```

## `ignore_hit`

Rejects the current candidate hit; the ray continues as if it was not there.

```dream
public extern fun ignore_hit(): void
```

## `accept_hit_and_end_search`

Accepts the current candidate hit and stops searching; the closest-hit shader runs for it.

```dream
public extern fun accept_hit_and_end_search(): void
```

## `report_hit`

Reports a hit on custom geometry at distance `t` with a `kind` from `0` to `127` and the `attributes` the hit shaders receive. Returns true when the hit was accepted.

```dream
public extern fun report_hit<A>(t: float, kind: int, attributes: A): bool
```
