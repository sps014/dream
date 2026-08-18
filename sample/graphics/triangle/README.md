# Colored triangle (`@vertex` / `@fragment`)

Minimal programmable graphics sample: a Dream `@vertex` + `@fragment` pair draws a
colored triangle to a canvas. Uses `@builtin("position")`, `@interpolate`,
`front_facing`, and `GpuRenderPipeline.create_ex`.

## Build

```bash
cargo run -- --runtime --web sample/graphics/triangle/triangle.dream
```

This writes `triangle.wasm`, `triangle.abi.json`, `triangle.wgsl`, and
`triangle.web.runtime.js` under `target/web/`. Open `triangle.html` only after
building — a missing `.wasm` is a 404.

## Run

### Native (wgpu)

```bash
cargo run -- run sample/graphics/triangle/triangle.dream
```

Native `dream run` executes real WGSL compute and render pipelines via wgpu and presents
through a winit window.

### Browser (WebGPU)

Serve the **sample directory** (uses `./target/web/triangle.web.runtime.js` + `./target/web/triangle.wasm`):

```bash
npx serve sample/graphics/triangle
# open http://localhost:3000/triangle.html
```

Or serve the **repository root** (falls back to `runtime/dream.js`):

```bash
npx serve .
# open http://localhost:3000/sample/graphics/triangle/triangle.html
```

Requires a WebGPU browser (Chrome/Edge). Build with `--runtime --web` first (see above).

## Shader API (happy path)

```dream
struct VsOut {
    @builtin("position")
    public clip: GpuVec4;
    @interpolate("perspective")
    public color: GpuVec4;
}

@vertex
fun tri_vs(v: Vertex): VsOut { /* … */ }

@fragment
fun tri_fs(v: VsOut): GpuVec4 {
    return v.color;
}

let desc = GpuRenderPipelineDesc.defaults();
let pipe = await GpuRenderPipeline.create_ex("tri_vs", "tri_fs", desc);
```

A field named `position: GpuVec4` is still accepted as sugar for `@builtin("position")`.

See [Vertex & fragment shaders](../../docs/reference/language/shaders.md) and [`system.gpu`](../../docs/reference/stdlib/gpu.md).
