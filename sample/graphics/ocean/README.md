# Dream ocean (WebGPU)

Gerstner-wave ocean mesh with fresnel water, sun specular, whitecap foam, and
distance haze — authored as Dream `@vertex` / `@fragment` shaders (same package
as [`triangle/`](../triangle/) and the compute fluid demo).

## Shaders (sketch)

```dream
@vertex
fun ocean_vs(v: Vertex, time: float, cam_y: float, cam_z: float, aspect: float): VsOut {
    // layered Gerstner displacement + analytic normals → clip space
}

@fragment
fun ocean_fs(v: VsOut): GpuVec4 {
    // fresnel sky mix + Blinn specular + foam + haze
}
```

Host loop:

```dream
let uniforms = Uniforms.pack_f32([t, 6.5, 16.0, aspect]);
let _ = await GpuRenderPass.draw_indexed_ex(
    surface, pipe, verts, indices, index_count, uniforms, sky
);
await surface.present();
await Gpu.frame();
```

## Build

```sh
cargo run -- --runtime --web sample/graphics/ocean/ocean.dream
```

## Run

Serve the **sample directory** (uses `./ocean.web.runtime.js` + `./ocean.wasm`):

```sh
npx serve sample/graphics/ocean
# open http://localhost:3000/ocean.html
```

Or serve the **repository root** (falls back to `runtime/dream.js`):

```sh
npx serve .
# open http://localhost:3000/sample/graphics/ocean/ocean.html
```

Requires a modern browser with WebGPU. Native `dream run` cannot present a canvas.
Build first — a missing `.wasm` is a 404.

## Notes

- Grid: 128×128 verts, indexed triangles (~32k tris).
- Five Gerstner layers (swell + chop); normals from tangent derivatives.
- Uniforms: `time`, camera height/distance, aspect — packed with `Uniforms.pack_f32`.
- Depth buffer / MSAA / SSR are not in the v1 GPU API yet; this sample stays a single
  opaque mesh with sky clear color.
