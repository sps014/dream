# CoopShapeAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `CoopShapeAttribute`

Gives the fixed dimensions of a cooperative vector or matrix parameter or field.  Write one number for a `GpuCoopVec` element count, or input and output sizes for a `GpuMlpWeights` layer: `@coop_shape(16, 32)` is a layer from 16 inputs to 32 outputs. On a `GpuBuffer<GpuCoopVec<T>>` it is the length of each element. The sizes must match the data uploaded from the CPU.

```dream
public struct CoopShapeAttribute
```

## `dimensions`

The dimensions, outermost first.

```dream
public dimensions: List<int>
```
