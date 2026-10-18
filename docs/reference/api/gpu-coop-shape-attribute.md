# CoopShapeAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `CoopShapeAttribute`

Gives the fixed dimensions of a cooperative vector or matrix parameter or field.  Write one number for a `GpuCoopVec` element count, or input and output sizes for a `GpuMlpWeights` layer: `@coop_shape(16, 32)` is a layer from 16 inputs to 32 outputs. Dispatching a layer of other sizes fails with `GpuError.InvalidResource`. On a `GpuBuffer<GpuCoopVec<T>>` parameter it is the length of each element, and the CPU passes a `GpuBuffer<T>` that holds that many values per element.

```dream
public struct CoopShapeAttribute
```

## `dimensions`

The dimensions, outermost first.

```dream
public dimensions: List<int>
```
