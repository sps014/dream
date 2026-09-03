# GPU: colored triangle

A **vertex + fragment** shader that draws one triangle. Open a window (native) or a canvas (browser).

```dream
import system;
import system.gpu;

struct Vertex {
    public pos: GpuVec2;
    public color: GpuVec4;
}

struct VsOut {
    @builtin("position")
    public clip: GpuVec4;
    @interpolate("perspective")
    public color: GpuVec4;
}

@vertex
fun tri_vs(v: Vertex): VsOut {
    let o = VsOut();
    o.clip = GpuVec4.of(v.pos.x, v.pos.y, 0.0, 1.0);
    o.color = v.color;
    return o;
}

@fragment
fun tri_fs(v: VsOut): GpuVec4 {
    return v.color;
}

fun make_vert(x: float, y: float, r: float, g: float, b: float): Vertex {
    let v = Vertex();
    v.pos = GpuVec2.of(x, y);
    v.color = GpuVec4.of(r, g, b, 1.0);
    return v;
}

async fun main(): void {
    if (await Gpu.try_init()).is_err() {
        System.println("WebGPU unavailable");
        return;
    }

    let surface_r = GpuSurface.create("c", 800, 600);
    if surface_r.is_err() { return; }
    let surface = surface_r.unwrap_or(GpuSurface());

    let desc = GpuRenderPipelineDesc.defaults();
    let pipe_r = await GpuRenderPipeline.create_ex("tri_vs", "tri_fs", desc);
    if pipe_r.is_err() { return; }
    let pipe = pipe_r.unwrap_or(GpuRenderPipeline());

    let verts = GpuBuffer<Vertex>.vertex_from([
        make_vert(-0.5, -0.5, 1.0, 0.2, 0.2),
        make_vert(0.5, -0.5, 0.2, 1.0, 0.2),
        make_vert(0.0, 0.5, 0.2, 0.2, 1.0),
    ]);

    while !surface.close_requested {
        let _ = await GpuRenderPass.draw(surface, pipe, verts, 3);
        let _ = await surface.present();
        await Gpu.frame();
    }
}
```

Full sample (input, depth options): [`sample/graphics/triangle/`](https://github.com/sps014/dream/tree/main/sample/graphics/triangle).

Compute instead of draw: [GPU SAXPY](gpu-saxpy.md). API: [shaders](../reference/language/shaders.md), [`system.gpu`](../reference/stdlib/gpu.md).
