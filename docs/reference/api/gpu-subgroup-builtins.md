# Subgroup built-ins

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `subgroup_size`

The number of threads in a subgroup, the group of threads the GPU runs in lockstep: for example 32 on NVIDIA, 32 or 64 on AMD and 32 on Apple GPUs. Subgroup functions need `GpuFeature.Subgroups` and are not available with `GpuTarget.Dx11`.

```dream
public extern fun subgroup_size(): uint
```

## `subgroup_invocation_id`

This thread's lane within its subgroup, from `0` to `subgroup_size() - 1`.

```dream
public extern fun subgroup_invocation_id(): uint
```

## `subgroup_add`

The sum of `value` over every active lane of the subgroup.

```dream
public extern fun subgroup_add<T>(value: T): T
```

## `subgroup_min`

The smallest `value` among the active lanes.

```dream
public extern fun subgroup_min<T>(value: T): T
```

## `subgroup_max`

The largest `value` among the active lanes.

```dream
public extern fun subgroup_max<T>(value: T): T
```

## `subgroup_prefix_add`

The sum of `value` over the active lanes below this one.

```dream
public extern fun subgroup_prefix_add<T>(value: T): T
```

## `subgroup_prefix_count`

The number of active lanes below this one where `condition` is true.

```dream
public extern fun subgroup_prefix_count(condition: bool): uint
```

## `subgroup_count`

The number of active lanes where `condition` is true.

```dream
public extern fun subgroup_count(condition: bool): uint
```

## `subgroup_ballot`

One bit per lane, set where `condition` is true.

```dream
public extern fun subgroup_ballot(condition: bool): GpuUVec4
```

## `subgroup_broadcast`

`value` from lane `lane`, the same lane for every thread.

```dream
public extern fun subgroup_broadcast<T>(value: T, lane: uint): T
```

## `subgroup_shuffle`

`value` from lane `lane`, which may differ per thread.

```dream
public extern fun subgroup_shuffle<T>(value: T, lane: uint): T
```

## `subgroup_all`

True when `condition` is true on every active lane.

```dream
public extern fun subgroup_all(condition: bool): bool
```

## `subgroup_any`

True when `condition` is true on any active lane.

```dream
public extern fun subgroup_any(condition: bool): bool
```

## `subgroup_elect`

True on exactly one active lane, the lowest.

```dream
public extern fun subgroup_elect(): bool
```
