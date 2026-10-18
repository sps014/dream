# GpuVec2

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuVec2`

A vector of two `float`s, laid out like a GPU `float2`.  Use it on the CPU and in shaders alike: in buffers, as shader inputs and outputs, and for texture coordinates. Arithmetic works component by component, and a `float` operand applies to every component.

```dream
public struct GpuVec2
```

## `x`

The first component.

```dream
public x: float
```

## `y`

The second component.

```dream
public y: float
```

## `constructor`

Creates the vector `(x, y)`.

```dream
public constructor(x: float, y: float)
```

## `constructor`

Creates a vector with both components equal to `value`.

```dream
public constructor(value: float)
```

## `dot`

The dot product: `x * other.x + y * other.y`.

```dream
public fun dot(other: GpuVec2): float
```

## `length`

The vector's Euclidean length.

```dream
public fun length(): float
```

## `normalize`

The vector scaled to length `1`. A zero vector gives non-finite components.

```dream
public fun normalize(): GpuVec2
```
