# GpuRay

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuRay`

A ray for `trace_ray` and `GpuRayQuery`: hits count only between distances `t_min` and `t_max` from `origin` along `direction`.

```dream
public struct GpuRay
```

## `origin`

Where the ray starts.

```dream
public origin: GpuVec3
```

## `direction`

The ray's direction; it need not have length `1`, and distances are measured in multiples of it.

```dream
public direction: GpuVec3
```

## `t_min`

The nearest distance at which hits count, usually a small positive value that keeps a surface from hitting itself.

```dream
public t_min: float
```

## `t_max`

The farthest distance at which hits count.

```dream
public t_max: float
```

## `constructor`

Creates a ray from `origin` along `direction` that counts hits from `t_min` to `t_max`.

```dream
public constructor(origin: GpuVec3, direction: GpuVec3, t_min: float, t_max: float)
```
