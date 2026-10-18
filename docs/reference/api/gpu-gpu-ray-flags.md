# GpuRayFlags

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuRayFlags`

Options for `trace_ray` and `GpuRayQuery`. Combine several with `|`.

```dream
public enum GpuRayFlags
```

## `None`

No options.

```dream
None = 0
```

## `ForceOpaque`

Treats all geometry as opaque, so any-hit shaders do not run.

```dream
ForceOpaque = 1
```

## `ForceNonOpaque`

Treats all geometry as non-opaque, so any-hit shaders run for every candidate.

```dream
ForceNonOpaque = 2
```

## `AcceptFirstHitAndEndSearch`

Stops at the first hit found instead of searching for the closest one, for shadow rays.

```dream
AcceptFirstHitAndEndSearch = 4
```

## `SkipClosestHitShader`

Skips the closest-hit shader.

```dream
SkipClosestHitShader = 8
```

## `CullBackFacingTriangles`

Ignores triangles facing away from the ray.

```dream
CullBackFacingTriangles = 16
```

## `CullFrontFacingTriangles`

Ignores triangles facing the ray.

```dream
CullFrontFacingTriangles = 32
```

## `CullOpaque`

Ignores opaque geometry.

```dream
CullOpaque = 64
```

## `CullNonOpaque`

Ignores non-opaque geometry.

```dream
CullNonOpaque = 128
```

## `SkipTriangles`

Ignores triangle geometry.

```dream
SkipTriangles = 256
```

## `SkipProceduralPrimitives`

Ignores custom geometry tested by intersection shaders.

```dream
SkipProceduralPrimitives = 512
```
