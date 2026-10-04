# Supply rendering data

A rendering function needs vertex data, textures, and other resources. Match the supplied data to its declared parameters.

[Back to overview](shaders.md)

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

`@interpolate` takes an optional second argument choosing where the varying is sampled, which only matters under MSAA:

- `"centroid"` — samples inside the covered part of the fragment (stops values being extrapolated past the edge of a partially covered triangle)
- `"sample"` — shades once per sample
- For `"flat"`: `"first"` or `"either"`, selecting which vertex provides the value

```dream
struct VsOut {
    public position: GpuVec4;
    @interpolate("perspective", "centroid") public uv: GpuVec2;
    @interpolate("flat", "first") public material: float;
}
```


## Vertex buffers

Every leading struct parameter of a `@vertex` function is one vertex buffer slot, in declaration order, matching `set_vertex_buffer(slot, …)`.
Attribute locations run across all of them, so a second buffer continues where the first stopped.
Mark a parameter `@instance` to step its buffer once per instance instead of once per vertex:

```dream
struct Vertex { public pos: GpuVec3; public uv: GpuVec2; }

struct Instance {
    public model_row0: GpuVec4;
    @format("unorm8x4") public tint: GpuVec4;
}

@vertex
fun mesh_vs(v: Vertex, @instance inst: Instance, mvp: GpuMat4): VsOut { /* … */ }
```

`@format` sets the *wire* format only: the buffer stores the packed bytes while the shader still reads the field's declared type, so `unorm8x4` turns a `GpuVec4` tint from 16 bytes into 4.
The format's shader type has to be the field's own type, which rules out reading `unorm8x4` as anything but a `GpuVec4`.
Attributes are otherwise packed tightly in declaration order.


## Resource bindings

Resource parameters (`GpuTexture`, `GpuSampler`, `GpuBuffer<T>`, and anything else, which becomes a uniform) get `@group(0)` and an auto-incrementing binding index per group.
Both can be named explicitly — one group per update frequency, built once and reused across frames:

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

All uniform parameters of one shader collapse into a single uniform block, so `@group` / `@binding` on any of them places the whole block; two uniform parameters asking for different slots is an error.

App-side, textures, samplers, and storage buffers are supplied with a `GpuBindList` in the same order the shader declares them (ascending group, then binding, separately per kind):

```dream
let binds = GpuBindList.begin().texture(albedo).sampler(samp);
let _ = GpuRenderPass.draw_ex(
    surface, pipe, verts, 3, uniforms, clear, Option.Some(binds)
).await;
```


## Sampling textures

`Gpu.texture_sample(tex, samp, u, v)` is the everyday filtered read.
Every one of these returns all four channels as a `GpuVec4` (except the depth comparisons, which return a single fraction):

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

`texture_gather` needs a literal `0`–`3` for its component.

### Shadow maps

A `@depth` texture read through a `@compare` sampler does the depth test in hardware and filters the four results, so one call returns how much of the texel neighbourhood the fragment is lit by:

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

`texture_sample_compare` is `@fragment` only; `texture_sample_compare_level` pins mip level 0 and works from `@compute` too.


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
