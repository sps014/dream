# Dream elevated (WebGPU) — terrain raymarch

Faithful port of [Inigo Quilez “Elevated”](https://www.shadertoy.com/view/MdX3Rr)
(CC BY-NC-SA 3.0): fullscreen fragment heightfield march (`interesct`),
derivative fbm terrain (`terrainH` / `M` / `L` octaves), rock/snow shading,
soft shadows, atmospheric fog, sun disk, and the original camera path.

Shadertoy’s `iChannel0` noise texture is a lattice hash in `[0,1]` (same interpolant
and analytic derivatives as `noised`). Ridge layout is therefore not pixel-identical
to the PNG, but lighting, snow, fog, sky, and fly-through match the shader.

[`@gpu`](../../../docs/reference/language/shaders.md#gpu-helpers) helpers hold noise / map / lighting.

## Build

```sh
cargo run -- --runtime --web sample/graphics/elevated/elevated.dream
```

## Run

```sh
npx serve sample/graphics/elevated
# open http://localhost:3000/elevated.html
```

Or from the repo root:

```sh
npx serve .
# open http://localhost:3000/sample/graphics/elevated/elevated.html
```

Native window:

```sh
cargo run -- run sample/graphics/elevated/elevated.dream
```

Requires WebGPU. Build first — a missing `.wasm` is a 404.

## Notes

- Fullscreen triangle + `@fragment` raymarch (6-octave heightfield, first-hit sphere trace).
- Gradient noise (not lattice value noise) so ridges aren't cubic cells.
- Uses `@builtin("position")`, `@gpu` helpers, and
  `GpuRenderPipeline.create_ex` with `GpuRenderPipelineDesc.defaults()`.
- Camera / sun / fog / vignette match the author’s Elevated.shader.
- Canvas backing store follows CSS/client size (same as native); not Retina DPR.
