# MissAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `MissAttribute`

Marks a miss shader, which runs when a ray hits nothing. Its only parameter is `ref` to the ray's payload struct, which it fills in, for example with a sky color.

```dream
public struct MissAttribute
```
