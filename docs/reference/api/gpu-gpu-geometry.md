# GpuGeometry

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuGeometry`

The geometry of one bottom-level acceleration structure: triangles from a vertex buffer, or boxes around custom shapes that an `@intersection` shader tests.

```dream
public class GpuGeometry
```

## `triangles`

Triangles whose corners are consecutive triples of `positions`.

```dream
public static fun triangles(positions: GpuBuffer<GpuVec3>): GpuGeometry
```

## `boxes`

Custom shapes, one per pair of `bounds` corners: each box is its minimum corner followed by its maximum corner.

```dream
public static fun boxes(bounds: GpuBuffer<GpuVec3>): GpuGeometry
```

## `indexed`

Builds triangles from consecutive triples of `indices` into the positions instead of consecutive positions, and returns this geometry. Only for `triangles`.

```dream
public fun indexed(indices: GpuBuffer<uint>): GpuGeometry
```

## `opaque`

Sets whether the geometry is opaque, and returns this geometry. Any-hit shaders only run for non-opaque geometry. Defaults to `true`.

```dream
public fun opaque(value: bool): GpuGeometry
```
