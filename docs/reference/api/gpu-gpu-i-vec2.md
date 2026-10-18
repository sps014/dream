# GpuIVec2

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuIVec2`

A vector of two `int`s, laid out like a GPU `int2`, for pixel coordinates and 2D sizes.

```dream
public struct GpuIVec2
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

## `constructor`

Creates the vector `(x, y)`.

```dream
public constructor(x: int, y: int)
```
