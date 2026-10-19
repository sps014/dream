# GpuRenderPass

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuRenderPass`

A render pass being recorded: draws into the targets given to `GpuCommandEncoder.begin_render_pass`, or into a window frame through `GpuFrame.pass`.  Set a pipeline, bind resources and buffers, then draw. Settings stay in effect for later draws in the same pass. Recording mistakes are reported by `GpuDevice.submit`, or by `GpuFrame.present` for frame passes.

```dream
public class GpuRenderPass
```

## `set_pipeline`

Uses `pipeline` for the following draws.

```dream
public fun set_pipeline(pipeline: GpuRenderPipeline): void
```

## `set_vertex_buffer`

Reads vertices for vertex buffer layout `slot` of the pipeline from `buffer`.

```dream
public fun set_vertex_buffer<T: unmanaged>(slot: int, buffer: GpuBuffer<T>): void
```

## `set_index_buffer`

Reads 32-bit indexes for `draw_indexed` from `buffer`.

```dream
public fun set_index_buffer(buffer: GpuBuffer<uint>): void
```

## `set_index_buffer`

Reads 16-bit indexes for `draw_indexed` from `buffer`.

```dream
public fun set_index_buffer(buffer: GpuBuffer<ushort>): void
```

## `set_viewport`

Maps clip space onto the rectangle at `x`, `y` of `width` by `height` pixels, with `(0, 0)` at the top-left of the target. Defaults to the whole target.

```dream
public fun set_viewport(x: float, y: float, width: float, height: float): void
```

## `set_scissor`

Discards pixels outside the rectangle at `x`, `y` of `width` by `height` pixels. Defaults to the whole target.

```dream
public fun set_scissor(x: int, y: int, width: int, height: int): void
```

## `bind`

Binds `texture` to the shader parameter `slot`.

```dream
public fun bind(slot: GpuBindingSlot, texture: GpuTexture): void
```

## `bind`

Binds `sampler` to the shader parameter `slot`.

```dream
public fun bind(slot: GpuBindingSlot, sampler: GpuSampler): void
```

## `bind` (acceleration structure)

Binds `scene` for inline ray queries in a native render shader. The device must
support `GpuFeature.RayQuery`; WebGPU does not support acceleration structures.
Build a top-level structure with `GpuDevice.build_top_level`, check the feature
before creating the pipeline, and bind the scene before drawing.

```dream
public fun bind(slot: GpuBindingSlot, scene: GpuAccelerationStructure): void
```

## `bind_buffer`

Binds `buffer` to the `GpuBuffer` shader parameter `slot`.

```dream
public fun bind_buffer<T: unmanaged>(slot: GpuBindingSlot, buffer: GpuBuffer<T>): void
```

## `bind_value`

Sets the scalar, vector, matrix or struct shader parameter `slot` to `value`.

```dream
public fun bind_value<T: unmanaged>(slot: GpuBindingSlot, value: T): void
```

## `draw`

Draws `vertex_count` vertices, `instance_count` times. The vertex shader sees `vertex_id()` from `0` and `instance_id()` from `0`.

```dream
public fun draw(vertex_count: int, instance_count: int = 1): void
```

## `draw_indexed`

Draws `index_count` vertices chosen by the index buffer, `instance_count` times.

```dream
public fun draw_indexed(index_count: int, instance_count: int = 1): void
```

## `end`

Ends the pass. Ending it twice does nothing.

```dream
public fun end(): void
```
