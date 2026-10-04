# GpuEncoder, GpuRenderPassEncoder

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GpuEncoder`

Records a whole frame — any number of render passes and draws — and submits it in one host call. ```dream let enc = GpuEncoder.begin(); let pass = enc.render_pass(GpuRenderTarget.surface(surface).clear(sky)); pass.set_pipeline(pipe).set_vertex_buffer(0, verts); for (let i = 0; i < meshes.length; i = i + 1) { pass.set_bind_group(1, meshes[i].material); pass.set_uniforms(meshes[i].uniforms); pass.draw(meshes[i].count, 1, 0, 0); } pass.end(); (enc.submit().await)?; ```

```dream
public class GpuEncoder
```

## `begin`

Starts an empty command stream.

```dream
public static fun begin(): GpuEncoder
```

## `render_pass`

Begins a render pass over `target`. Call `end()` on the returned recorder before starting another pass; passes may not be interleaved.

```dream
public fun render_pass(target: GpuRenderTarget): GpuRenderPassEncoder
```

## `write_timestamp`

Writes a GPU timestamp between passes. Requires `timestamp_query_inside_encoders`.

```dream
public fun write_timestamp(queries: GpuQuerySet, index: int): GpuEncoder
```

## `submit`

Replays every recorded pass into a single host command buffer and submits it.

```dream
public async fun submit(token: Option<CancellationToken> = Option.None): Result<bool, GpuError>
```

## `class GpuRenderPassEncoder`

Records draw commands for one render pass. Mirrors WebGPU's `GPURenderPassEncoder`.

```dream
public class GpuRenderPassEncoder
```

## `set_pipeline`

```dream
public fun set_pipeline(p: GpuRenderPipeline): GpuRenderPassEncoder
```

## `set_bind_group`

```dream
public fun set_bind_group(group: int, bg: GpuBindGroup): GpuRenderPassEncoder
```

## `set_bind_list`

Supplies resources for every group the pipeline declares at once, consumed in shader declaration order per kind. Convenient for a one-off draw; a `GpuBindGroup` is cheaper per frame because the host resolves it only once.

```dream
public fun set_bind_list(binds: GpuBindList): GpuRenderPassEncoder
```

## `set_uniforms`

Sets the uniform block for the following draws (`Uniforms.pack` / `pack_f32`). Each call takes a fresh slot in the pipeline's uniform pool, so draws in one pass can differ.

```dream
public fun set_uniforms(uniforms: byte[]): GpuRenderPassEncoder
```

## `set_vertex_buffer`

```dream
public fun set_vertex_buffer<T : unmanaged>(slot: int, b: GpuBuffer<T>): GpuRenderPassEncoder
```

## `set_index_buffer`

`Uint32` indices are one `int` each, so `GpuBuffer<int>` is the natural carrier. `Uint16` has no Dream element type, so pack the pairs into a `GpuBuffer<byte>` yourself — halving index memory is worth the packing for a large mesh.

```dream
public fun set_index_buffer<T : unmanaged>(b: GpuBuffer<T>, format: GpuIndexFormat): GpuRenderPassEncoder
```

## `set_viewport`

Viewport in framebuffer pixels; `min_depth`/`max_depth` are normally `0.0` / `1.0`.

```dream
public fun set_viewport( x: float, y: float, width: float, height: float, min_depth: float, max_depth: float ): GpuRenderPassEncoder
```

## `set_scissor`

```dream
public fun set_scissor(x: int, y: int, width: int, height: int): GpuRenderPassEncoder
```

## `draw`

```dream
public fun draw( vertex_count: int, instance_count: int, first_vertex: int, first_instance: int ): GpuRenderPassEncoder
```

## `draw_indexed`

```dream
public fun draw_indexed( index_count: int, instance_count: int, first_index: int, base_vertex: int, first_instance: int ): GpuRenderPassEncoder
```

## `draw_indirect`

Reads draw arguments from `b` at `byte_offset` (4 x u32: vertex_count, instance_count, first_vertex, first_instance).

```dream
public fun draw_indirect<T : unmanaged>(b: GpuBuffer<T>, byte_offset: int): GpuRenderPassEncoder
```

## `draw_indexed_indirect`

Reads indexed draw arguments from `b` at `byte_offset` (5 x u32: index_count, instance_count, first_index, base_vertex, first_instance).

```dream
public fun draw_indexed_indirect<T : unmanaged>( b: GpuBuffer<T>, byte_offset: int ): GpuRenderPassEncoder
```

## `write_timestamp`

Writes a GPU timestamp inside this pass. Requires `timestamp_query_inside_passes`.

```dream
public fun write_timestamp(queries: GpuQuerySet, index: int): GpuRenderPassEncoder
```

## `end`

```dream
public fun end(): void
```
