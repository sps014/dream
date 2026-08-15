# Vertex & fragment shaders (`@vertex` / `@fragment`)

Dream can compile ordinary-looking functions into **WebGPU vertex and fragment
shaders** (WGSL), the same way [`@compute`](compute.md) becomes a compute kernel.
Mark top-level functions with `@vertex` / `@fragment`, link them with
`GpuRenderPipeline.create` / `create_ex`, and draw through `GpuRenderPass`.

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
let pipe = await GpuRenderPipeline.create("tri_vs", "tri_fs");
let verts = GpuBuffer<Vertex>.vertex_from([/* … */]);
let _ = await GpuRenderPass.draw(surface, pipe, verts, 3);
let _ = await surface.present();
```

Depth-tested mesh with blending / cull:

```dream
let desc = GpuRenderPipelineDesc.mesh();
let pipe = await GpuRenderPipeline.create_ex("vs", "fs", desc);
let depth = GpuTexture.depth24(width, height);
let _ = await GpuRenderPass.draw_instanced(
    surface, pipe, verts, vertex_count, instance_count,
    uniforms, clear, Option.Some(depth), GpuLoadOp.Clear
);
```

## Attributes

| Need | Default | Optional |
|------|---------|----------|
| Stage | — | **`@vertex` / `@fragment` required** |
| Shared helpers | — | **`@gpu`** on ordinary functions called from shaders |
| Attribute / varying slots | Field order → `0, 1, 2…` | `@location(N)` to remap |
| Clip position | Field named **`position: GpuVec4`** | or `@builtin("position")` on any `GpuVec4` field |
| Interpolation | perspective | `@interpolate("flat"\|"linear"\|"perspective")` |
| Fragment color | Return **`GpuVec4`** | or an output struct with `@location` colors (+ optional `@builtin("frag_depth")`) |
| Bindings | auto `@group(0)` | `@group(N)` / `@binding(N)` on resource params |

## Fragment outputs (MRT)

```dream
struct FsOut {
    @location(0) public color: GpuVec4;
    @location(1) public aux: GpuVec4;
}

@fragment
fun fs(v: VsOut): FsOut {
    let o = FsOut();
    o.color = v.color;
    o.aux = GpuVec4.of(frag_coord.x, frag_coord.y, 0.0, 1.0);
    return o;
}
```

## Builtins

- Vertex: `vertex_index`, `instance_index`
- Fragment: `frag_coord`, `front_facing`; `sample_index` / `primitive_index` when referenced
  (the latter emits `enable primitive_index;`)

## Vector math

`GpuVec2` / `GpuVec3` / `GpuVec4` support WGSL-shaped arithmetic inside shaders
(and the same expressions on the CPU):

- `v + w`, `v - w`, `v * w`, `v / w` (component-wise)
- `v * s`, `v / s`, `s * v` (and `s + v`, `v + s`, …)
- `-v`
- `GpuMatN * GpuVecN` and `GpuMatN * GpuMatN`

`GpuMath.mix` / `min` / `max` / `abs` / `clamp` / `saturate` / `sign` / `floor` /
`ceil` / `fract` / `sqrt` / `exp` / `pow` take `float` or `GpuVecN`. `normalize` /
`length` / `dot` use the same name for vec2/3/4. `GpuVecN.splat(s)` is WGSL `vecN(s)`.
`GpuMat2.of(c0, c1)` lowers to `mat2x2<f32>(c0, c1)`. `GpuMath.transpose` maps to WGSL
`transpose` (overloaded for `GpuMat2` / `GpuMat3` / `GpuMat4`). Shader `let` is inferred
from the initializer — `let s = GpuMath.sin(t)` needs no `: float`.

```dream
@gpu
fun shade(a: GpuVec3, b: GpuVec3, t: float): GpuVec3 {
    return GpuMath.mix(a, b, t) * 0.8 + GpuVec3.splat(0.1);
}
```

## `@gpu` helpers

Shaders may only call other GPU stages / `@gpu` helpers and `GpuMath` / `GpuVec*` /
`GpuMat*` builtins. Helpers are emitted as WGSL `fn`s and are **not** callable from CPU code.

```dream
@gpu
fun sea_octave(ux: float, uz: float, choppy: float): float {
    return GpuMath.pow(1.0 - GpuMath.pow(0.5, 0.65), choppy);
}
```

Rules: top-level only; not generic/async/extern; explicit non-void return type.

## Matrices

`GpuMat2` / `GpuMat3` / `GpuMat4` (column-major). Prefer `m * v` / `m * n` in shaders;
`GpuMath.mul` is the named form (mat×vec or mat×mat). `GpuMath.transpose` maps to WGSL
`transpose`.

## Rules

- Top-level only; not async, generic, or extern; body skipped for MIR/WASM (emitted as WGSL).
- `@vertex` returns a value struct with a position builtin plus varyings.
- `@fragment` first parameter is usually that interface struct; return `GpuVec4` or an output struct.
- When names passed to `create` / `create_ex` are **string literals**, the compiler checks stages
  and matching interface types.
- `sizeof(T)` lowers to a WGSL integer literal; `nameof(...)` is not available in shader bodies
  (it produces `string`). Details: [Operators — sizeof and nameof](operators.md#sizeof-and-nameof).

## Related

- [Compute shaders](compute.md)
- [`system.gpu` API](../stdlib/gpu.md)
- Samples: [`triangle/`](https://github.com/sps014/dream/tree/main/sample/graphics/triangle), [`ocean/`](https://github.com/sps014/dream/tree/main/sample/graphics/ocean), [`elevated/`](https://github.com/sps014/dream/tree/main/sample/graphics/elevated)
