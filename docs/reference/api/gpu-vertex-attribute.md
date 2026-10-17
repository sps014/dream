# VertexAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `VertexAttribute`

Marks a function as a vertex shader, which runs once per vertex of a draw and returns the vertex's clip-space position plus any values the fragment shader reads.  The return type is a struct whose fields carry `@builtin("position")` or `@location(n)`. The shader becomes `GpuShaders.<name>`, which `GpuDevice.render_pipeline` accepts.

```dream
public struct VertexAttribute
```
