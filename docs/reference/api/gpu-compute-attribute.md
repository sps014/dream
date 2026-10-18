# ComputeAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `ComputeAttribute`

Marks a function as a compute shader, an entry point the GPU runs once per thread.  `x`, `y` and `z` are the workgroup size: how many threads run together and can share workgroup memory. `y` and `z` default to `1`, so `@compute(64)` is a 64-thread line of threads. The shader generator checks the body against the shader rules, compiles it for every target listed in `@gpu_targets`, and makes it available as `GpuShaders.<name>.dispatch(device, ...)`. The function itself can no longer be called from CPU code.

```dream
public struct ComputeAttribute
```

## `x`

Threads per workgroup along x.

```dream
public x: int
```

## `y`

Threads per workgroup along y; `1` when omitted.

```dream
public y: Option<int>
```

## `z`

Threads per workgroup along z; `1` when omitted.

```dream
public z: Option<int>
```
