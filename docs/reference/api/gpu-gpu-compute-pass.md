# GpuComputePass

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuComputePass`

A compute pass being recorded by `GpuCommandEncoder.begin_compute_pass`. Generated `GpuShaders.<name>.encode` calls record dispatches into it; dispatches in one pass run in order, each seeing the previous ones' writes.

```dream
public class GpuComputePass
```

## `end`

Ends the pass. Ending it twice does nothing.

```dream
public fun end(): void
```
