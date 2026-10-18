# GpuWorkgroupArray

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuWorkgroupArray`

Fast memory shared by the threads of one compute workgroup, created with `workgroup_array<T>(length)`. Threads exchange values through it; call `workgroup_barrier()` between writing and reading another thread's elements.

```dream
public struct GpuWorkgroupArray<T>
```

## `length`

The number of elements.

```dream
public extern fun length(): int
```

## `this`

Reads element `index`.

```dream
public extern fun this[index: int]: T
```

## `this`

Writes element `index`.

```dream
public extern fun this[index: int] = value: T
```
