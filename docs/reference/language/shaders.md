# Vertex & fragment shaders (`@vertex` / `@fragment`)

Rendering uses a vertex function to position shapes and a fragment function to choose the pixels' colors. Write them in Dream with `@vertex` and `@fragment`, then connect them through a rendering pipeline.

## Explore this topic

- [Supply rendering data](shader-resources.md)
- [Write rendering functions](shader-functions.md)

## Execution model

| Target | What runs |
|--------|-----------|
| Browser (`dream.js` + WebGPU) | Real WGSL compute + render |
| Native `dream run` (wgpu) | Real WGSL compute + render; window present via winit |

Golden tests that dispatch kernels require a GPU adapter (e.g. Metal on macOS).
Headless CI without an adapter should skip or expect `gpu unavailable` from `try_init`.

## Quick start

```dream
import system;
import system.gpu;

struct Vertex {
    public pos: GpuVec2;
    public color: GpuVec4;
}

struct VsOut {
    public position: GpuVec4; // sugar for @builtin("position")
    @interpolate("perspective")
    public color: GpuVec4;
}

@vertex
fun tri_vs(v: Vertex): VsOut {
    let o = VsOut();
    o.position = GpuVec4.of(v.pos.x, v.pos.y, 0.0, 1.0);
    o.color = v.color;
    return o;
}

@fragment
fun tri_fs(v: VsOut): GpuVec4 {
    return v.color;
}
```

Host (browser):

```dream
let pipe = GpuRenderPipeline.create("tri_vs", "tri_fs").await;
let verts = GpuBuffer<Vertex>.vertex_from([/* … */]);
let _ = GpuRenderPass.draw(surface, pipe, verts, 3).await;
let _ = surface.present().await;
```

Depth-tested mesh with blending / cull:

```dream
let desc = GpuRenderPipelineDesc.mesh();
let pipe = GpuRenderPipeline.create_ex("vs", "fs", desc).await;
let depth = GpuTexture.depth24(width, height);
let _ = GpuRenderPass.draw_instanced(
    surface, pipe, verts, vertex_count, instance_count,
    uniforms, clear, Option.Some(depth), GpuLoadOp.Clear
).await;
```

## Related

- [Compute shaders](compute.md)
- [`system.gpu` API](../stdlib/gpu.md)
- Samples: [`triangle/`](https://github.com/sps014/dream/tree/main/sample/graphics/triangle), [`ocean/`](https://github.com/sps014/dream/tree/main/sample/graphics/ocean), [`elevated/`](https://github.com/sps014/dream/tree/main/sample/graphics/elevated)
