# DifferentiableAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `DifferentiableAttribute`

Makes a `@gpu` helper differentiable, so `GpuDiff.forward` and `GpuDiff.backward` can compute its derivatives automatically.  Every function it calls must itself be differentiable: another `@differentiable` helper or a differentiable built-in such as `GpuMath.sin`, `GpuMath.exp` or `GpuMath.max`. Other calls are build errors at the call. Works on every target, including `WebGpu`.

```dream
public struct DifferentiableAttribute
```
