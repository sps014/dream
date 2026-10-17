# RayPayloadAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `RayPayloadAttribute`

Marks a field of a ray payload struct, the data a ray carries between `trace_ray` and the hit and miss shaders. Only marked fields travel with the ray, so keep payloads small.

```dream
public struct RayPayloadAttribute
```
