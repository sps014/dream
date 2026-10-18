# GpuAccelerationStructure

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuAccelerationStructure`

A ray tracing index over geometry. A bottom-level structure holds one mesh; a top-level structure holds placed instances of bottom-level ones and is what shaders trace rays against. Build them with `GpuDevice.build_bottom_level` and `build_top_level`.

```dream
public class GpuAccelerationStructure
```
