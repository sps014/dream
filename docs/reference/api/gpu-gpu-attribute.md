# GpuAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuAttribute`

Marks a helper function that shaders may call.  A helper follows the same rules as a shader body and may use every shader built-in, but it is not an entry point and has no `GpuShaders` descriptor. Like shaders, helpers are compiled for the GPU only and cannot be called from CPU code.

```dream
public struct GpuAttribute
```
