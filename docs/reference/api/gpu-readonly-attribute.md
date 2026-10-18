# ReadonlyAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `ReadonlyAttribute`

Marks a shader's `GpuBuffer` parameter as read-only. The shader can read its elements but not write them, which lets the driver skip synchronization and lets several shaders read the same buffer at once. Writing to it is a build error.

```dream
public struct ReadonlyAttribute
```
