# GpuDispatch

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuDispatch`

How many threads a compute dispatch runs.  The `threads_*` constructors take the number of threads you need; the dispatch rounds up to whole workgroups of the shader's `@compute` size, so a shader should check its `global_id()` against the real count. `groups` takes workgroup counts directly.

```dream
public struct GpuDispatch
```

## `threads_1d`

Runs at least `count` threads along x.

```dream
public static fun threads_1d(count: int): GpuDispatch
```

## `threads_2d`

Runs at least `width` by `height` threads, for example one per pixel.

```dream
public static fun threads_2d(width: int, height: int): GpuDispatch
```

## `threads_3d`

Runs at least `x` by `y` by `z` threads.

```dream
public static fun threads_3d(x: int, y: int, z: int): GpuDispatch
```

## `groups`

Runs exactly `x` by `y` by `z` workgroups.

```dream
public static fun groups(x: int, y: int, z: int): GpuDispatch
```

## `group_counts`

The workgroup counts for a shader whose `@compute` size is `workgroup`. Thread counts round up to whole workgroups; zero or negative counts give zero workgroups, which runs nothing.

```dream
public fun group_counts(workgroup: GpuIVec3): GpuIVec3
```
