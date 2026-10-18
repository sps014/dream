# GpuTriangleHit

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuTriangleHit`

Where a ray hit a triangle, received by `@closest_hit` and `@any_hit` shaders.

```dream
public struct GpuTriangleHit
```

## `barycentrics`

The weights of the triangle's second and third corners at the hit point; the first corner's weight is `1 - barycentrics.x - barycentrics.y`.

```dream
public barycentrics: GpuVec2
```
