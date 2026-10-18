# GpuVec2

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

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

## `of`

Creates the vector `(x, y)`.

```dream
public static fun of(x: float, y: float): GpuVec2
```

## `splat`

Creates a vector with both components equal to `value`.

```dream
public static fun splat(value: float): GpuVec2
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
