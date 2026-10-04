# GpuRenderPass

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GpuRenderPass`

One-draw convenience helpers over `GpuEncoder`. Each call records a single-pass, single-draw command stream and submits it; reach for `GpuEncoder` directly when a frame has more than one draw, since these submit once per call.

```dream
public class GpuRenderPass
```

## `blit`

Blits `source` onto `surface` (fullscreen textured triangle).

```dream
public static async fun blit(surface: GpuSurface, source: GpuTexture, token: Option<CancellationToken> = Option.None): Result<bool, GpuError>
```

## `draw`

Clears the surface (black) and draws `vertex_count` vertices with `pipeline`.

```dream
public static async fun draw<T : unmanaged>( surface: GpuSurface, pipeline: GpuRenderPipeline, vertices: GpuBuffer<T>, vertex_count: int, token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `draw_ex`

Like `draw`, with shader uniforms (`Uniforms.pack` / `pack_f32`) and a clear color. `binds` supplies the textures / samplers / storage buffers the shaders declare in group 0.

```dream
public static async fun draw_ex<T : unmanaged>( surface: GpuSurface, pipeline: GpuRenderPipeline, vertices: GpuBuffer<T>, vertex_count: int, uniforms: byte[], clear: GpuVec4, binds: Option<GpuBindList> = Option.None, token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `draw_instanced`

Draw with instancing, optional depth attachment, and load-op control.

```dream
public static async fun draw_instanced<T : unmanaged>( surface: GpuSurface, pipeline: GpuRenderPipeline, vertices: GpuBuffer<T>, vertex_count: int, instance_count: int, uniforms: byte[], clear: GpuVec4, depth: Option<GpuTexture>, load_op: GpuLoadOp, binds: Option<GpuBindList> = Option.None, token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `draw_indexed`

Clears the surface and draws indexed geometry (indices are `int` → WebGPU `uint32`).

```dream
public static async fun draw_indexed<T : unmanaged>( surface: GpuSurface, pipeline: GpuRenderPipeline, vertices: GpuBuffer<T>, indices: GpuBuffer<int>, index_count: int, token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `draw_indexed_ex`

Like `draw_indexed`, with uniforms, clear color, and shader resource bindings.

```dream
public static async fun draw_indexed_ex<T : unmanaged>( surface: GpuSurface, pipeline: GpuRenderPipeline, vertices: GpuBuffer<T>, indices: GpuBuffer<int>, index_count: int, uniforms: byte[], clear: GpuVec4, binds: Option<GpuBindList> = Option.None, token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `draw_indexed_instanced`

Indexed draw with instancing, optional depth, and load-op control.

```dream
public static async fun draw_indexed_instanced<T : unmanaged>( surface: GpuSurface, pipeline: GpuRenderPipeline, vertices: GpuBuffer<T>, indices: GpuBuffer<int>, index_count: int, instance_count: int, uniforms: byte[], clear: GpuVec4, depth: Option<GpuTexture>, load_op: GpuLoadOp, binds: Option<GpuBindList> = Option.None, token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `draw_to`

Draws into an offscreen color texture (optional depth), not the swapchain.

```dream
public static async fun draw_to<T : unmanaged>( color: GpuTexture, depth: Option<GpuTexture>, pipeline: GpuRenderPipeline, vertices: GpuBuffer<T>, vertex_count: int, instance_count: int, uniforms: byte[], clear: GpuVec4, load_op: GpuLoadOp, binds: Option<GpuBindList> = Option.None, token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```
