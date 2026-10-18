# GpuVertexLayout

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuVertexLayout`

How the vertex shader's `@location` inputs are read from one vertex buffer.  Start from `GpuVertexLayout(...)` with the byte size of one element, usually `sizeof` of the vertex struct, and add one `attribute` per input.

```dream
public class GpuVertexLayout
```

## `constructor`

Creates a vertex layout with the supplied byte stride.

```dream
public constructor(stride: int)
```

## `stride`

Bytes from one element to the next.

```dream
public get stride(): int
```

## `is_per_instance`

Whether the buffer advances once per instance instead of once per vertex.

```dream
public get is_per_instance(): bool
```

## `attribute_count`

The number of attributes added.

```dream
public get attribute_count(): int
```

## `attribute`

Feeds the vertex shader input `@location(location)` from `offset` bytes into each element, read as `format`. Returns this layout.

```dream
public fun attribute(location: int, format: GpuVertexFormat, offset: int): GpuVertexLayout
```

## `per_instance`

Sets whether the buffer advances once per instance, for per-instance data such as transforms, and returns this layout.

```dream
public fun per_instance(value: bool): GpuVertexLayout
```
