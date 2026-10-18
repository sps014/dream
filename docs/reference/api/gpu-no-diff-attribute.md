# NoDiffAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `NoDiffAttribute`

Excludes a parameter of a `@differentiable` helper from differentiation. Its derivative is treated as zero and it takes a plain value instead of a `GpuDiffPair`.

```dream
public struct NoDiffAttribute
```
