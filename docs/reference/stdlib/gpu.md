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
| `Gpu.is_available()` | adapter present? |
| `await Gpu.try_init()` | request device (call once) |
| `Gpu.ready()` | init succeeded |
| `await Gpu.frame()` | wait a display frame |
| `await Gpu.timestamp()` | GPU timestamp |

`GpuError` implements [`Error`](option-result.md). Headless machines often have no adapter.

## Buffers

`GpuBuffer<T>.alloc(n)`, `.from(data)`, `.vertex_from(data)`. Then `.length`, `write` / `write_at`, `await read` / `read_at`, `copy_to`. `GpuSwap<T>` is a front/back pair (`swap()`).

## Dispatch (`@compute`)

`Compute.run_1d(name, buffers, count)`, `run_2d` / `run_3d`, `run_2d_uniforms`, `run_resources`, `dispatch_indirect`, `run_shader`. Bind with `GpuBindList`. Pack CPU values with `Uniforms.pack`. `ComputePass` batches several dispatches then `submit()`.

## Textures, surfaces, draw

`GpuTexture.rgba8` (and depth / float / cube variants), `GpuSampler.linear()` / `nearest()`. `GpuSurface.create` / `from_canvas`, `present()`, input helpers, `GpuRenderPass.draw` / `blit`. Vertex path: `GpuRenderPipeline.create_ex`, `GpuVec2` / `GpuVec4`, `@builtin("position")`.

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
