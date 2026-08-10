# Dream ocean (WebGPU) — Seascape raymarch

Faithful port of [Alexander Alekseev / TDM “Seascape”](https://www.shadertoy.com/view/Ms2SD1):
fullscreen fragment raymarch (`heightMapTracing`), choppy `sea_octave` height field,
fresnel sky reflection + water refraction, horizon sky mix, soft gamma.

A displaced mesh cannot match Shadertoy; this sample uses the same tracing approach.

[`@gpu`](../../../docs/language/shaders.md#gpu-helpers) helpers hold noise / map / lighting.

## Build

```sh
cargo run -- --runtime --web sample/graphics/ocean/ocean.dream
```

## Run

```sh
npx serve sample/graphics/ocean
# open http://localhost:3000/ocean.html
```

Or from the repo root:

```sh
npx serve .
# open http://localhost:3000/sample/graphics/ocean/ocean.html
```

Requires WebGPU. Build first — a missing `.wasm` is a 404.

## Notes

- Fullscreen triangle + `@fragment` raymarch (32 height steps, 3/5 octaves).
- Uses `@builtin("position")`, `@gpu` helpers, `GpuMath.saturate`, and
  `GpuRenderPipeline.create_ex` with `GpuRenderPipelineDesc.overlay()`.
- Camera / `SEA_TIME` / colors match the author’s Seascape.shader.
- Canvas backing store uses `devicePixelRatio` (see `ocean.html`).
