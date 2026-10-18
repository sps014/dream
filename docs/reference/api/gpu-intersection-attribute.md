# IntersectionAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `IntersectionAttribute`

Marks an intersection shader, which tests a ray against custom geometry such as spheres inside axis-aligned boxes. It reports hits with `report_hit`.

```dream
public struct IntersectionAttribute
```
