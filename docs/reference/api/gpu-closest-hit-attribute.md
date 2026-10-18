# ClosestHitAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `ClosestHitAttribute`

Marks a closest-hit shader, which runs once for the nearest surface a ray hit.  Its first parameter is `ref` to the ray's payload struct, which it fills in; its second receives the hit's attributes, such as `GpuTriangleHit` for triangle geometry. Use it in a `GpuHitGroup`.

```dream
public struct ClosestHitAttribute
```
