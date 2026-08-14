# `system.gpu`

WebGPU compute and draw from Dream. Auto-imported when you write `@compute`, `@vertex`, or `@fragment`. You can also `import system.gpu;`.

Language: [Compute shaders](../language/compute.md), [Vertex & fragment](../language/shaders.md).

Cookbook: [GPU SAXPY](../../cookbook/gpu-saxpy.md), [GPU triangle](../../cookbook/gpu-triangle.md).

```dream
import system;
import system.gpu;

async fun main(): void {
    if ((await Gpu.try_init()).is_err()) {
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

Kernel-only: `GpuMath`, `Gpu.workgroup_barrier` / `storage_barrier`, `Gpu.atomic_*`.

Native `dream run` uses wgpu; the browser uses `navigator.gpu`. More samples: [`life/`](https://github.com/sps014/dream/tree/main/sample/compute/life), [`fluid/`](https://github.com/sps014/dream/tree/main/sample/fluid), [`ocean/`](https://github.com/sps014/dream/tree/main/sample/graphics/ocean), [`elevated/`](https://github.com/sps014/dream/tree/main/sample/graphics/elevated).
