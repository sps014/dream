# Colored triangle (`@vertex` / `@fragment`)

Minimal programmable graphics sample: a Dream `@vertex` + `@fragment` pair draws a
colored triangle to a canvas.

## Build

```bash
cargo run -- --runtime --web sample/graphics/triangle/triangle.dream
```

This writes `triangle.wasm`, `triangle.abi.json`, `triangle.wgsl`, and
`triangle.web.runtime.js` next to the `.dream` file. Open `triangle.html` only after
building — a missing `.wasm` is a 404.

## Run

Serve the **sample directory** (uses `./triangle.web.runtime.js` + `./triangle.wasm`):

```bash
npx serve sample/graphics/triangle
# open http://localhost:3000/triangle.html
```

Or serve the **repository root** (falls back to `runtime/dream.js`):

```bash
npx serve .
# open http://localhost:3000/sample/graphics/triangle/triangle.html
```

Requires a WebGPU browser (Chrome/Edge). Native `dream run` stages buffers but does
not execute render pipelines; use the browser path.

## Shader API (happy path)

Only `@vertex` / `@fragment` are required. Locations are declaration order; clip
position is the field named `position: GpuVec4`.

```dream
struct Vertex {
    public pos: GpuVec2;
    public color: GpuVec4;
}

struct VsOut {
    public position: GpuVec4;
    public color: GpuVec4;
}

@vertex
fun tri_vs(v: Vertex): VsOut { /* … */ }

@fragment
fun tri_fs(v: VsOut): GpuVec4 {
    return v.color;
}
```

See [Vertex & fragment shaders](../../docs/language/shaders.md) and [`system.gpu`](../../docs/stdlib/gpu.md).
