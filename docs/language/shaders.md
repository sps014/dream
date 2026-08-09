# Vertex & fragment shaders (`@vertex` / `@fragment`)

Dream can compile ordinary-looking functions into **WebGPU vertex and fragment
shaders** (WGSL), the same way [`@compute`](compute.md) becomes a compute kernel.
Mark top-level functions with `@vertex` / `@fragment`, link them with
`GpuRenderPipeline.create`, and draw through `GpuRenderPass.draw`.

Native `dream run` does **not** execute render pipelines; use a browser with
WebGPU (see [stdlib GPU](../stdlib/gpu.md) and
[`sample/graphics/triangle/`](https://github.com/sps014/dream/tree/main/sample/graphics/triangle)).

## Quick start

```dream
import system;
import system.gpu;

struct Vertex {
    public pos: GpuVec2;
    public color: GpuVec4;
}

struct VsOut {
    public position: GpuVec4; // reserved name → clip-space builtin
    public color: GpuVec4;    // varying (location 0 by order)
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

## Attributes (minimal)

| Need | Default | Optional |
|------|---------|----------|
| Stage | — | **`@vertex` / `@fragment` required** |
| Attribute / varying slots | Field order → `0, 1, 2…` | `@location(N)` to remap |
| Clip position | Field named **`position: GpuVec4`** on the VS return struct | — |
| Fragment color | Return **`GpuVec4`** | — |

Do not put `@location` / `@builtin` on the happy path unless you need a remap.

## Rules

- Top-level only; not async, generic, or extern; body skipped for MIR/WASM (emitted as WGSL).
- `@vertex` returns a value struct with exactly one `position: GpuVec4` plus varyings.
- `@fragment` first parameter is that same interface struct; return type is `GpuVec4`.
- Injected builtins: `vertex_index` / `instance_index` (`int`) in VS; `frag_coord` (`GpuVec4`) in FS.
- Cannot call `@vertex` / `@fragment` like CPU functions — use `GpuRenderPipeline.create`.
- When both names passed to `create` are **string literals**, the compiler checks that the
  shaders exist, have the right stages, and share the same interface struct.

## Vectors

### `GpuVec2` / `GpuVec3` / `GpuVec4`

Unmanaged float vectors in `system.gpu`. Inside `@vertex` / `@fragment` / `@compute` they
lower to WGSL `vecN<f32>`. Layout for vertex buffers: 8 / 12 / 16 bytes (tight floats).

```dream
let p = GpuVec3.of(0.0, 1.0, 0.0);
let c = GpuVec4.of(0.1, 0.4, 0.8, 1.0);
```

`GpuMath.normalize` / `dot` / `cross` / `length` / `reflect` / `mix` / `pow` map to WGSL
builtins inside shaders.

### `GpuId3`

Integer XYZ id for `@compute` builtins (`global_id`, …).

## Related

- [Compute shaders](compute.md)
- [`system.gpu` API](../stdlib/gpu.md)
- Samples: [`triangle/`](https://github.com/sps014/dream/tree/main/sample/graphics/triangle), [`ocean/`](https://github.com/sps014/dream/tree/main/sample/graphics/ocean)
