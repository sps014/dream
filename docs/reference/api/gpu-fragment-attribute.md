# FragmentAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `FragmentAttribute`

Marks a function as a fragment shader, which runs once per covered pixel and returns its color.  Its parameter is usually the struct the vertex shader returns; fields marked `@location(n)` arrive interpolated across the triangle. The shader becomes `GpuShaders.<name>`, which `GpuDevice.render_pipeline` accepts.

```dream
public struct FragmentAttribute
```
