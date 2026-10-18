# GpuUVec4

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuUVec4`

A vector of four `uint`s, laid out like a GPU `uint4`. `subgroup_ballot` returns one bit per subgroup lane in it, lane 0 in the lowest bit of `x`.

```dream
public struct GpuUVec4
```

## `x`

The first component.

```dream
public x: uint
```

## `y`

The second component.

```dream
public y: uint
```

## `z`

The third component.

```dream
public z: uint
```

## `w`

The fourth component.

```dream
public w: uint
```

## `constructor`

Creates the vector `(x, y, z, w)`.

```dream
public constructor(x: uint, y: uint, z: uint, w: uint)
```
