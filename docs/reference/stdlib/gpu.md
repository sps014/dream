# `system.gpu`

WebGPU compute and draw from Dream. Auto-imported when you write `@compute`, `@vertex`, or `@fragment`. You can also `import system.gpu;`.

Language: [Compute shaders](../language/compute.md), [Vertex & fragment](../language/shaders.md).

Cookbook: [GPU SAXPY](../../cookbook/gpu-saxpy.md), [GPU triangle](../../cookbook/gpu-triangle.md).

```dream
import system;
import system.gpu;

async fun main(): void {
    if (await Gpu.try_init()).is_err() {
        System.println("gpu unavailable");
        return;
    }
}
```

## Device and time

| Call | Meaning |
| --- | --- |
| `Gpu.is_available` | adapter present? |
| `await Gpu.try_init()` | request device (high-performance adapter) |
| `await Gpu.try_init(GpuPowerPreference.LowPower)` | same, preferring battery-friendly GPUs |
| `Gpu.ready` | init succeeded (false after device-lost until `try_init` again) |
| `Gpu.check()` | pending uncaptured error or device-lost, else `Ok` |
| `await Gpu.frame()` | wait a display frame |
| `await Gpu.timestamp()` | GPU timestamp |
| `Gpu.capabilities()` | optional features and limits the device got |

## Capabilities

WebGPU only lets a shader or resource touch a feature the *device* opted into when it was created;
reaching for an un-requested one is device-loss-grade rather than a recoverable validation error.
Dream requests every optional feature the adapter offers, so `Gpu.capabilities()` is the
authoritative answer to "may I use this?" — gate on it rather than trying and recovering.

Everything reads as `false` / `0` until `try_init` succeeds, since capabilities describe the
negotiated device rather than the raw adapter.

```dream
let caps = Gpu.capabilities();
if caps.texture_compression_astc {
    // mobile-sized asset path
}
let tile = caps.max_invocations_per_workgroup;
```

| Field | Meaning |
| --- | --- |
| `shader_float16` | half-precision arithmetic in shaders |
| `subgroup`, `subgroup_barrier` | subgroup intrinsics; the barrier is native-only |
| `min_subgroup_size`, `max_subgroup_size` | subgroup width range, `0` when unreported |
| `tile_float16`, `tile_float`, `tile_n` | cooperative-matrix tiles; reserved, always off today |
| `texture_compression_bc` / `_etc2` / `_astc` | compressed format families (BC desktop, ETC2/ASTC mobile) |
| `depth32_float_stencil8`, `float32_filterable` | the matching optional formats |
| `max_buffer_bytes`, `max_storage_binding_bytes` | allocation and storage-binding ceilings |
| `max_workgroup_storage_bytes` | `var<workgroup>` bytes per workgroup |
| `max_invocations_per_workgroup`, `max_workgroup_size_x/y/z` | `@workgroup_size` ceilings |
| `max_workgroups_per_dimension` | per-dimension dispatch ceiling |

Buffer sizes and the compute workgroup limits are raised to the adapter maximum; every other limit
stays at the portable WebGPU default, so a program developed against a large GPU still runs on a
small one. Creating a `Bc*` / `Etc2*` / `Astc*` texture without the matching flag fails with
`GpuError.unsupported`.

`GpuError` implements [`Error`](option-result.md). Headless machines often have no adapter. Async GPU methods take an optional last `token`; cancelled `Result` calls return `GpuError` `ECANCELLED`. A lost device (`DEVICE_LOST`) is distinct from `VALIDATION`: recover with `await Gpu.try_init()` and recreate GPU resources. `Gpu.check()` drains a pending lost / uncaptured event without waiting for the next submit.

`Gpu.try_init(GpuPowerPreference.LowPower)` (or `Default`) is only consulted when no device is alive yet; after a loss, a different preference re-picks the adapter. `try_init()` keeps high-performance.

Swapchain drawable size is CSS/logical pixels unless `GpuSurfaceDesc.max_pixel_ratio` is greater than `1`: then `width`/`height` become `client × min(devicePixelRatio, max_pixel_ratio)` (typical game clamp is `2`). Read the used scale with `surface.pixel_ratio` and the uncapped window/DPR with `surface.scale_factor`. `request_pointer_lock()` feeds relative `dx`/`dy` for FPS cameras; `request_fullscreen()` is borderless. Both need a user gesture in the browser. `pointers()` is the multi-touch list (`pointer()` stays the primary latch). Gamepad sticks still poll via `gamepad_axis`; `poll_events` also yields `GamepadAxis` when a value changes.

## Buffers

`GpuBuffer<T>.alloc(n)`, `.from(data)`, `.vertex_from(data)`. Then `.length`, `write` / `write_at`, `await read` / `read_at`, `copy_to`. `GpuSwap<T>` is a front/back pair (`swap()`).

## Dispatch (`@compute`)

`Compute.run_1d(name, buffers, count)`, `run_2d` / `run_3d`, `run_2d_uniforms`, `run_resources`, `dispatch_indirect`, `run_shader`. Bind with `GpuBindList`. Pack CPU values with `Uniforms.pack`. `ComputePass` batches several dispatches then `submit()`.

## Textures, surfaces, draw

`GpuTexture.rgba8` (and depth / float / cube variants), `await GpuTexture.from_image_bytes(png_or_jpeg)` for PNG/JPEG decode, `GpuSampler.linear()` / `nearest()`. `GpuSurface.create` / `from_canvas`, `configure(w, h)` or `configure(GpuSurfaceDesc)` (`present_mode`, `alpha_mode`, `color_space`, `max_pixel_ratio`), `present()`, input helpers (`pointer()`, `pointers()`, pointer lock, fullscreen, gamepad axes), `GpuRenderPass.draw` / `blit`. Vertex path: `GpuRenderPipeline.create_ex`, `GpuVec2` / `GpuVec4`, `@builtin("position")`.

Kernel-only: `GpuMath`, `Gpu.workgroup_barrier` / `storage_barrier`, `Gpu.atomic_*` (`atomic_load`, `atomic_store`, `atomic_add`, `atomic_sub`, `atomic_min`, `atomic_max`, `atomic_and`, `atomic_or`, `atomic_xor`, `atomic_exchange`), `Gpu.dpdx` / `dpdy` / `fwidth` (derivatives), `Gpu.texture_*` (`texture_dimensions`, `texture_sample_cube`, `texture_load`, `texture_store`, `texture_sample`).

## Vector math

`GpuVec2` / `GpuVec3` / `GpuVec4` are packed float vectors (`vecN<f32>` in WGSL). The
same operators work in `@compute` / `@vertex` / `@fragment` and on the CPU:

| Expression | Meaning (WGSL) |
| --- | --- |
| `v + w`, `v - w`, `v * w`, `v / w` | component-wise |
| `v * s`, `s * v`, `v / s` | scale / divide by `float` |
| `s + v`, `v + s`, `v - s`, `s - v`, `s / v` | scalar on either side |
| `-v` | negate |
| `m * v`, `m * n` | `GpuMatN` × vector / matrix |
| `GpuVecN.of(...)` | `vecN(x, y, …)` |
| `GpuVecN.splat(s)` | `vecN(s)` |
| `GpuMatN.of(c0, …)` | `matNxN(c0, …)` column-major |
| `GpuMatN.identity()` | identity matrix |
| `GpuMat4.perspective(fov_y, aspect, near, far)` | WebGPU clip Z in `[0, 1]`, `fov_y` in radians |
| `GpuMat4.ortho(l, r, b, t, near, far)` | orthographic projection |
| `GpuMat4.look_at(eye, center, up)` | right-handed view matrix |
| `GpuMat4.translation` / `rotation` / `scaling` | TRS builders |
| `GpuMat3.normal_matrix(m)` | inverse-transpose of `m`'s upper 3×3 |
| `GpuQuat.xyzw` / `from_axis_angle` / `rotate` / `to_mat4` | CPU-side rotation (turn into a matrix for shaders) |

`GpuMath` overloads (same names as the scalar builtins):

| Call | Vector args |
| --- | --- |
| `mix(a, b, t)` | `vec, vec, float` or `vec, vec, vec` |
| `min` / `max` | `vec, vec` |
| `abs` / `sign` / `floor` / `ceil` / `fract` / `sqrt` / `exp` / `exp2` / `log2` / `round` / `trunc` / `radians` / `degrees` / `saturate` | `vec` |
| `clamp(x, lo, hi)` | `vec, float, float` or `vec, vec, vec` |
| `pow(x, e)` | `vec, float` or `vec, vec` |
| `normalize` / `length` / `dot` | `GpuVec2` / `GpuVec3` / `GpuVec4` |
| `distance(a, b)` | `GpuVec2` / `GpuVec3` / `GpuVec4` |
| `refract(i, n, eta)` / `faceforward(n, i, nref)` | `GpuVec3` |
| `transpose` / `inverse` | `GpuMat2` / `GpuMat3` / `GpuMat4` |
| `determinant` | `GpuMat2` / `GpuMat3` / `GpuMat4` |
| `mul` | `GpuMatN × GpuVecN` or `GpuMatN × GpuMatN` |
| `count_one_bits` / `reverse_bits` / `count_leading_zeros` / `count_trailing_zeros` | `int` bitwise |

Near-zero `normalize` on the CPU returns a unit axis (`(1,0)`, `(0,1,0)`, or `(0,0,0,1)`);
shaders use WGSL `normalize`.

Native `dream run` uses wgpu; the browser uses `navigator.gpu`. More samples: [`life/`](https://github.com/sps014/dream/tree/main/sample/compute/life), [`fluid/`](https://github.com/sps014/dream/tree/main/sample/fluid), [`ocean/`](https://github.com/sps014/dream/tree/main/sample/graphics/ocean), [`elevated/`](https://github.com/sps014/dream/tree/main/sample/graphics/elevated).
