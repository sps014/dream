# GpuRenderPipelineDesc

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuRenderPipelineDesc`

The fixed-function settings of a render pipeline: target format, primitive topology, culling, blending, depth testing and vertex buffer layouts.  Start from `GpuRenderPipelineDesc.defaults` and chain the settings you want to change.

```dream
public class GpuRenderPipelineDesc
```

## `defaults`

Settings for drawing triangle lists into a `color_format` target: no culling, no blending, no depth buffer, no vertex buffers and one sample per pixel. Pass a surface's `GpuSurface.color_format` to draw into a window.

```dream
public static fun defaults(color_format: GpuTextureFormat): GpuRenderPipelineDesc
```

## `color_format`

The color target's format.

```dream
public get color_format(): GpuTextureFormat
```

## `topology`

How vertices form primitives.

```dream
public get topology(): GpuTopology
```

## `cull_mode`

Which triangles are skipped.

```dream
public get cull_mode(): GpuCullMode
```

## `blend_mode`

How new colors combine with the target.

```dream
public get blend_mode(): GpuBlendMode
```

## `depth_format`

The depth buffer's format, or `None` without depth testing.

```dream
public get depth_format(): Option<GpuTextureFormat>
```

## `sample_count`

Samples per pixel; must match the pass targets' sample count.

```dream
public get sample_count(): int
```

## `vertex_buffer_count`

The number of vertex buffer layouts added.

```dream
public get vertex_buffer_count(): int
```

## `with_topology`

Sets how vertices form primitives, and returns this description.

```dream
public fun with_topology(topology: GpuTopology): GpuRenderPipelineDesc
```

## `with_cull_mode`

Sets which triangles are skipped, and returns this description.

```dream
public fun with_cull_mode(mode: GpuCullMode): GpuRenderPipelineDesc
```

## `with_blend_mode`

Sets how new colors combine with the target, and returns this description.

```dream
public fun with_blend_mode(mode: GpuBlendMode): GpuRenderPipelineDesc
```

## `with_depth`

Enables depth testing against a `format` depth buffer: a fragment is kept when `compare` passes against the stored depth, and its depth is stored when `write` is true. Returns this description.

```dream
public fun with_depth(
    format: GpuTextureFormat,
    compare: GpuCompare,
    write: bool
): GpuRenderPipelineDesc
```

## `with_samples`

Sets the samples per pixel, `1` or `4`, and returns this description.

```dream
public fun with_samples(count: int): GpuRenderPipelineDesc
```

## `with_vertex_buffer`

Adds a vertex buffer layout. The first layout added reads from vertex buffer slot `0`, the next from slot `1`, and so on. Returns this description.

```dream
public fun with_vertex_buffer(layout: GpuVertexLayout): GpuRenderPipelineDesc
```
