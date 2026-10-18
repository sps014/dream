# GpuIVec3

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuIVec3`

A vector of three `int`s, laid out like a GPU `int3`. Thread and workgroup ids such as `global_id()` use it, so components index buffers and compare with `int` counts directly.

```dream
public struct GpuIVec3
```

## `x`

The first component.

```dream
public x: int
```

## `y`

The second component.

```dream
public y: int
```

## `z`

The third component.

```dream
public z: int
```

## `of`

Creates the vector `(x, y, z)`.

```dream
public static fun of(x: int, y: int, z: int): GpuIVec3
```

## `xy`

The first two components, for example a pixel coordinate from a 2D dispatch.

```dream
public get xy(): GpuIVec2
```
