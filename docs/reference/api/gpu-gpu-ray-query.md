# GpuRayQuery

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuRayQuery`

An inline ray trace inside an ordinary shader, without a ray tracing pipeline. Useful for shadows and ambient occlusion in compute and fragment shaders.  Create one with `GpuRayQuery.of`, start it with `trace`, then call `proceed` in a loop until it returns false. Inside the loop, decide about non-opaque candidates with `commit_triangle`; afterwards read the committed hit. Needs `GpuFeature.RayQuery`.

```dream
public struct GpuRayQuery
```

## `constructor`

Creates a query with `flags` applied to every trace.

```dream
public constructor(flags: GpuRayFlags)
```

## `trace`

Starts tracing `ray` against `scene`, seeing only instances whose mask shares a bit with `mask`.

```dream
public extern fun trace(scene: GpuAccelerationStructure, mask: uint, ray: GpuRay): void
```

## `proceed`

Advances to the next candidate that needs a decision; false once the trace finished.

```dream
public extern fun proceed(): bool
```

## `commit_triangle`

Accepts the current non-opaque triangle candidate as a hit.

```dream
public extern fun commit_triangle(): void
```

## `candidate_distance`

The distance to the current candidate.

```dream
public extern fun candidate_distance(): float
```

## `candidate_barycentrics`

The current triangle candidate's barycentric weights, as in `GpuTriangleHit`.

```dream
public extern fun candidate_barycentrics(): GpuVec2
```

## `has_hit`

True when the finished trace committed a hit.

```dream
public extern fun has_hit(): bool
```

## `hit_distance`

The distance to the committed hit.

```dream
public extern fun hit_distance(): float
```

## `hit_barycentrics`

The committed hit's barycentric weights, as in `GpuTriangleHit`.

```dream
public extern fun hit_barycentrics(): GpuVec2
```

## `hit_instance_id`

The `GpuInstance.with_id` value of the committed hit's instance.

```dream
public extern fun hit_instance_id(): int
```

## `hit_primitive_index`

The index of the committed hit's triangle or box within its geometry.

```dream
public extern fun hit_primitive_index(): int
```
