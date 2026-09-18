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

## Attributes

| Need | Default | Optional |
|------|---------|----------|
| Stage | — | **`@vertex` / `@fragment` required** |
| Shared helpers | — | **`@gpu`** on ordinary functions called from shaders |
| Attribute / varying slots | Field order → `0, 1, 2…` | `@location(N)` to remap |
| Vertex buffer slots | One per leading `@vertex` struct param, in order | `@instance` on a param to step it per instance |
| Attribute wire format | Matches the field's type | `@format("unorm8x4")` etc. to pack it smaller |
| Clip position | Field named **`position: GpuVec4`** | or `@builtin("position")` on any `GpuVec4` field |
| Interpolation | perspective | `@interpolate("flat"\|"linear"\|"perspective")`, plus an optional sampling qualifier |
| Fragment color | Return **`GpuVec4`** | or an output struct with `@location` colors (+ optional `@builtin("frag_depth")` / `@builtin("sample_mask")`) |
| Bindings | auto `@group(0)`, binding index auto-assigned per group | `@group(N)` / `@binding(N)` on resource params |
| `GpuTexture` binding | sampled `texture_2d<f32>` | `@storage` for a writable storage texture; `@cube` for `texture_cube<f32>` |
| `GpuBuffer<T>` binding | `read_write` storage | `@readonly` for read-only storage |

`@interpolate` takes an optional second argument choosing where the varying is sampled, which
only matters under MSAA. `"centroid"` samples inside the covered part of the fragment, which
stops values being extrapolated past the edge of a partially covered triangle; `"sample"`
additionally shades once per sample. For `"flat"` the second argument is `"first"` or `"either"`,
selecting which vertex provides the value:

```dream
struct VsOut {
    public position: GpuVec4;
    @interpolate("perspective", "centroid") public uv: GpuVec2;
    @interpolate("flat", "first") public material: float;
}
```

## Vertex buffers

Every leading struct parameter of a `@vertex` function is one vertex buffer slot, in declaration
order, matching `set_vertex_buffer(slot, …)`. Attribute locations run across all of them, so a
second buffer continues where the first stopped. Mark a parameter `@instance` to step its buffer
once per instance instead of once per vertex:

```dream
struct Vertex { public pos: GpuVec3; public uv: GpuVec2; }

struct Instance {
    public model_row0: GpuVec4;
    @format("unorm8x4") public tint: GpuVec4;
}

@vertex
fun mesh_vs(v: Vertex, @instance inst: Instance, mvp: GpuMat4): VsOut { /* … */ }
```

`@format` sets the *wire* format only: the buffer stores the packed bytes while the shader still
reads the field's declared type, so `unorm8x4` turns a `GpuVec4` tint from 16 bytes into 4. The
format's shader type has to be the field's own type, which rules out reading `unorm8x4` as anything
but a `GpuVec4`. Attributes are otherwise packed tightly in declaration order.

## Resource bindings

Resource parameters (`GpuTexture`, `GpuSampler`, `GpuBuffer<T>`, and anything else, which becomes a
uniform) get `@group(0)` and an auto-incrementing binding index per group. Both can be named
explicitly, which is what a material system needs — one group per update frequency, built once and
reused across frames:

```dream
@fragment
fun pbr_fs(
    input: VsOut,
    @group(0) @binding(0) camera: Camera,
    @group(1) @binding(0) albedo: GpuTexture,
    @group(1) @binding(1) samp: GpuSampler,
    @group(2) @binding(0) @readonly lights: GpuBuffer<Light>,
): GpuVec4 {
    return Gpu.texture_sample(albedo, samp, input.uv.x, input.uv.y);
}
```

All uniform parameters of one shader collapse into a single WGSL uniform block, so `@group` /
`@binding` on any of them places the whole block; two uniform parameters asking for different slots
is an error.

App-side, textures, samplers, and storage buffers are supplied with a `GpuBindList` in the same
order the shader declares them (ascending group, then binding, separately per kind):

```dream
let binds = GpuBindList.begin().texture(albedo).sampler(samp);
let _ = GpuRenderPass.draw_ex(
    surface, pipe, verts, 3, uniforms, clear, Option.Some(binds)
).await;
```

## Sampling textures

`Gpu.texture_sample(tex, samp, u, v)` is the everyday filtered read, and every one of these returns
all four channels as a `GpuVec4` (except the depth comparisons, which return a single fraction):

| Call | WGSL | Notes |
|---|---|---|
| `texture_sample` | `textureSample` | `@fragment` only; picks the mip level from derivatives |
| `texture_sample_level` | `textureSampleLevel` | explicit mip level, so also usable from `@compute` |
| `texture_sample_bias` | `textureSampleBias` | `@fragment` only; shifts the chosen mip level |
| `texture_sample_grad` | `textureSampleGrad` | supplies the derivatives by hand |
| `texture_sample_cube` | `textureSample` | samples a `@view("cube")` texture along a direction |
| `texture_sample_layer` | `textureSample` | one layer of a `@view("2d-array")` texture |
| `texture_gather` | `textureGather` | one component of the four texels filtering would blend |
| `texture_load` / `_level` / `_layer` | `textureLoad` | unfiltered texel fetch by integer coordinate |
| `texture_num_levels` / `_num_layers` | `textureNumLevels` / `Layers` | `_num_layers` needs an array texture |

`texture_gather` needs a literal `0`–`3` for its component, because WGSL requires a constant there.

### Shadow maps

A `@depth` texture read through a `@compare` sampler does the depth test in hardware and filters
the four results, so one call returns how much of the texel neighbourhood the fragment is lit by:

```dream
@fragment
fun lit_fs(
    v: VsOut,
    @group(0) @binding(0) @depth shadow: GpuTexture,
    @group(0) @binding(1) @compare shadow_samp: GpuSampler
): GpuVec4 {
    let lit = Gpu.texture_sample_compare(shadow, shadow_samp, v.uv.x, v.uv.y, v.light_depth);
    return GpuVec4.of(lit, lit, lit, 1.0);
}
```

`texture_sample_compare` is `@fragment` only; `texture_sample_compare_level` pins mip level 0 and
works from `@compute` too.

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
- Fragment: `frag_coord`, `front_facing`; `sample_index` / `primitive_index` / `sample_mask` when
  referenced (`primitive_index` emits `enable primitive_index;`)

`sample_mask` is the incoming coverage mask: bit *N* is set when sample *N* of this fragment is
covered, so `GpuMath.count_one_bits(sample_mask)` counts covered samples.

An output struct can also **write** `@builtin("sample_mask")` as an `int` field, which is how
alpha-to-coverage and custom MSAA masking work — clearing a bit discards that sample, and
clearing every bit discards the fragment:

```dream
struct FsOut {
    @location(0) public color: GpuVec4;
    @builtin("sample_mask") public coverage: int;
}
```

## Control flow

`if` / `while` / `do`-`while` / `for` / `switch` all work, including `break` and `continue`.
Two limits come from WGSL:

- **No loop labels.** `break outer;` / `continue outer;` are rejected, because WGSL's `break` and
  `continue` always apply to the innermost loop. Use a flag local, or move the inner loop into a
  [`@gpu` helper](#gpu-helpers) and `return` from it.
- **`switch` subjects are evaluated once** and case labels must be constant — a literal or a
  C-style enum member. Cases do not fall through, and a `break` inside a case body belongs to the
  enclosing loop, not to the `switch`.

C-style enums are usable in shaders; members fold to their integer value:

```dream
enum Mode { Add = 0, Mul = 1, Sub = 2 }

@gpu
fun apply(mode: int, a: float, b: float): float {
    switch (mode) {
        case Mode.Add: return a + b;
        case Mode.Mul: return a * b;
        default: return a - b;
    }
}
```

## Packing

`GpuMath` packs normalized vectors into a 32-bit `int` and back, matching the WGSL builtins of
the same name. These run on the CPU too, so a mesh packed on the host unpacks in a shader to the
same bits — useful for halving the size of vertex colours and normals, or for compact G-buffers:

| Pack | Unpack | Component range |
|---|---|---|
| `pack4x8unorm(GpuVec4)` | `unpack4x8unorm(int)` | `[0, 1]` × 4 bytes |
| `pack4x8snorm(GpuVec4)` | `unpack4x8snorm(int)` | `[-1, 1]` × 4 bytes |
| `pack2x16unorm(GpuVec2)` | `unpack2x16unorm(int)` | `[0, 1]` × 2 halves |
| `pack2x16snorm(GpuVec2)` | `unpack2x16snorm(int)` | `[-1, 1]` × 2 halves |

Components are clamped before scaling, and `x` occupies the low bits.

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
`GpuMat2.of(c0, c1)` becomes WGSL `mat2x2<f32>(c0, c1)`. `GpuMath.transpose` maps to WGSL
`transpose` (overloaded for `GpuMat2` / `GpuMat3` / `GpuMat4`), and `GpuMath.inverse` /
`GpuMath.determinant` cover all three sizes. Shader `let` is inferred from the initializer —
`let s = GpuMath.sin(t)` needs no `: float`.

### Swizzles

Reading two to four components at once gives a smaller (or reordered) vector, as in WGSL:

```dream
let rgb  = color.xyz;   // GpuVec3
let uv   = p.xy;        // GpuVec2
let flip = p.wzyx;      // GpuVec4, reversed
let grey = c.xxx;       // GpuVec3, broadcast
```

`rgba` is an interchangeable spelling for `xyzw` (`color.rgb` is `color.xyz`), but the two cannot
be mixed in one name. Components past the end of the source are an error: `.xyz` on a `GpuVec2`
does not compile.

The same expressions work on the CPU, where a swizzle builds a new `GpuVecN` and so reads its
receiver once per component. On the CPU the receiver therefore has to be something re-readable — a
local or a field path. Bind anything else to a local first:

```dream
let n = GpuMath.normalize(v);   // on the CPU, `GpuMath.normalize(v).xyz` is an error
let dir = n.xyz;
```

Inside shaders there is no such restriction, because the swizzle becomes a native WGSL one that
evaluates its receiver a single time.

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

Rules: top-level only; not generic/async/extern; explicit non-void return type. Parameters carry
values, so a `GpuBuffer` / `GpuTexture` / `GpuSampler` cannot be one — those are bound to a stage,
not passed. Index the resource in the stage function and pass the element, or make the helper its
own `@compute` kernel with its own binding.

## Matrices

`GpuMat2` / `GpuMat3` / `GpuMat4` (column-major). Prefer `m * v` / `m * n` in shaders;
`GpuMath.mul` is the named form (mat×vec or mat×mat). `GpuMath.transpose` maps to WGSL
`transpose`.

## Rules

- Top-level only; not async, generic, or extern; the body becomes a WGSL shader.
- `@vertex` returns a value struct with a position builtin plus varyings.
- `@fragment` first parameter is usually that interface struct; return `GpuVec4` or an output struct.
- When names passed to `create` / `create_ex` are **string literals**, Dream checks stages
  and matching interface types.
- `sizeof(T)` becomes a number in the shader; `nameof(...)` is not available in shader bodies
  (it produces `string`). Details: [Operators — sizeof and nameof](operators.md#sizeof-and-nameof).

## Related

- [Compute shaders](compute.md)
- [`system.gpu` API](../stdlib/gpu.md)
- Samples: [`triangle/`](https://github.com/sps014/dream/tree/main/sample/graphics/triangle), [`ocean/`](https://github.com/sps014/dream/tree/main/sample/graphics/ocean), [`elevated/`](https://github.com/sps014/dream/tree/main/sample/graphics/elevated)
