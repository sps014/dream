---
title: GPU and ML
summary: Write shaders in Dream and run compute, graphics, and small neural networks on the GPU.
icon: material/expansion-card
api: reference/api/system-gpu.md
---

# GPU programming

Import `system.gpu` and mark a function with a stage attribute such as `@compute`. The build compiles it for the GPU APIs you choose and generates a `GpuShaders` class with typed parameters.

Automatic differentiation and small networks are in [Machine learning](machine-learning.md).

## Setup

Building a program that contains shaders needs the pinned Slang shader compiler. Install it once:

```bash
dreamer toolchain install slang
```

Programs without shaders, such as one that only lists devices, build without it.

On Windows, DXIL targets also need the Windows SDK's DirectX compiler.

- `dreamer toolchain install slang` copies `dxcompiler.dll` and `dxil.dll` from the newest installed Windows 10 SDK into the Slang tool root.
- A Visual Studio Developer shell's `WindowsSdkVerBinPath` is used when it is set.
- If neither that variable nor the registry has the libraries, the install prints where to find the SDK.

Those libraries stay on the build machine. Ray queries need Shader Model 6.5, and bindless handles need 6.6.

At run time, native programs use the GPU drivers already on the machine. Browser builds use the
browser's WebGPU support; include the JavaScript host with `--runtime --web` (see
[JavaScript interop](../../language/interop/index.md)).

## A first compute shader

This program doubles a list of numbers on the GPU and adds one to each:

```dream
import system;
import system.gpu;

@compute(64, 1, 1)
fun scale_add(
    @readonly xs: GpuBuffer<float>,
    @readonly ys: GpuBuffer<float>,
    out: GpuBuffer<float>,
    a: float,
    n: int
): void {
    let i = global_id().x;
    if i < n {
        out[i] = a * xs[i] + ys[i];
    }
}

async fun run(): Result<Unit, GpuError> {
    let device = GpuDevice.create(GpuBackend.Auto).await?;

    let n = 1000;
    let xs = [0.0f; n];
    let ys = [0.0f; n];
    for (let i = 0; i < n; i = i + 1) {
        xs[i] = (float)i;
        ys[i] = 1.0f;
    }

    let x = device.buffer_from(xs)?;
    let y = device.buffer_from(ys)?;
    let out = device.buffer_alloc<float>(n)?;

    GpuShaders.scale_add.dispatch(device, GpuDispatch.threads_1d(n), x, y, out, 2.0f, n).await?;

    let values = out.read().await?;
    System.println(values[999]);
    return Result.Ok(Unit());
}

async fun main(): void {
    switch (run().await) {
        Ok(_) => {}
        Err(e) => {
            System.println("GPU error: " + e.message());
        }
    }
}
```

It prints `1999`. The steps are:

1. `@compute(64, 1, 1)` makes `scale_add` a compute shader that runs in groups of 64 threads.
   Each thread handles one element: `global_id().x` is its index across the whole dispatch.

2. `GpuDevice.create` opens a GPU. Every buffer and pipeline belongs to the device that made it.

3. `buffer_from` copies an array into GPU memory, and `buffer_alloc` reserves an empty buffer.

4. `GpuShaders.scale_add.dispatch` runs the shader. Arguments after the device and dispatch size match the shader parameters, and `threads_1d(n)` still needs an `i < n` check for a partly filled last group.

5. `read` copies the buffer back into a Dream array after the GPU finishes.

`@readonly` tells the compiler that a shader never writes a buffer. It is optional, but it lets
the GPU skip work and documents intent.

## Choosing GPU APIs

A shader is compiled for WebGPU unless you say otherwise, and that default runs in browsers and in native programs. To use the platform API, list targets with `@gpu_targets` on a shader or on the module:

```dream
@gpu_targets(GpuTarget.Metal, GpuTarget.Vulkan, GpuTarget.Dx12, GpuTarget.WebGpu)
module renderer;

import system.gpu;
```

| Target | API | Platforms |
|---|---|---|
| `WebGpu` | WebGPU | browsers, Node, and native programs everywhere |
| `Metal` | Metal | macOS |
| `Vulkan` | Vulkan | Windows, Linux, and macOS through MoltenVK |
| `Dx12` | Direct3D 12 | Windows |
| `Dx11` | Direct3D 11 | Windows, built on Windows |
| `Cuda` | CUDA | NVIDIA GPUs on Windows and Linux, compute shaders only |

Each build keeps the listed targets it can compile.

- A wasm build keeps `WebGpu`. A macOS build keeps `Metal`, `Vulkan`, and `WebGpu`.
- A shader that fits none of those targets is left out, with a warning. Calling it returns `GpuError.BackendNotCompiled`.
- `@gpu_targets` on a function replaces the module list for that shader.
- Set targets in code. There is no command-line flag or `dream.toml` setting.

Most targets need nothing beyond `dreamer toolchain install slang`. Building `Dx12` and `Dx11`
code also needs Microsoft's DirectX shader compilers (`dxc` and `fxc`, from the Windows SDK) on the
build machine, and `Cuda` needs NVIDIA's NVRTC library from the CUDA toolkit.

## Opening a device

`GpuDevice.create(GpuBackend.Auto)` picks the platform's native API when the build has shader code
for it, and WebGPU otherwise. Ask for a specific backend to test or compare APIs:

```dream
import system;
import system.gpu;

async fun main(): void {
    for (let backend in GpuShaders.backends()) {
        switch (GpuDevice.create(backend).await) {
            Ok(device) => {
                System.println(device.backend.to_string() + " is available");
            }
            Err(GpuError.BackendUnavailable(reason)) => {
                System.println(backend.to_string() + " cannot run here: " + reason);
            }
            Err(e) => {
                System.println(e.message());
            }
        }
    }
}

@compute(1, 1, 1)
fun touch(out: GpuBuffer<int>): void {
    out[0] = 1;
}
```

`GpuShaders.backends()` lists the backends this build has shader code for. A specific backend is
never replaced by another one: when its code is missing, `create` returns
`GpuError.BackendNotCompiled`, and when the machine cannot run it, for example CUDA without an
NVIDIA driver, `create` returns `GpuError.BackendUnavailable`.

Check `device.supports(GpuFeature.RayQuery)` before using an optional feature. A pipeline that needs a missing feature returns `GpuError.UnsupportedCapability`.

## Buffers

`GpuBuffer<T>` holds elements of any `unmanaged` type: numbers, `bool`, GPU vectors such as
`GpuVec3`, and structs made of them.

| Call | Effect |
|---|---|
| `device.buffer_from(values)` | A new buffer holding a copy of `values` |
| `device.buffer_alloc<T>(n)` | A new buffer of `n` zeroed elements |
| `buffer.write(values)` | Replaces the whole contents; `values` must have exactly `length` elements |
| `buffer.write_at(offset, values)` | Replaces the elements starting at element `offset` |
| `buffer.read().await` | Copies every element back, after submitted work that writes the buffer |
| `buffer.destroy()` | Frees GPU memory once submitted work is done, instead of when the last reference goes away |

On WebGPU, a `write_at` must start at a multiple of 4 bytes and end at one or at the end of the
buffer. That matters for buffers of `byte` and `float16`.

Scalars, `GpuVec2`, and `GpuVec4` have the same layout in Dream and in the shader. `GpuVec3` and struct fields can be padded on the GPU, so prefer `GpuVec4` and scalar fields for data both sides read.

## Recording several dispatches

`dispatch` submits one shader and waits for it. To run several shaders back to back without waiting
in between, record them into one command encoder and submit it once:

```dream
import system;
import system.gpu;

@compute(64, 1, 1)
fun add_one(values: GpuBuffer<int>, n: int): void {
    let i = global_id().x;
    if i < n {
        values[i] = values[i] + 1;
    }
}

async fun run(device: GpuDevice, values: GpuBuffer<int>, n: int): Result<Unit, GpuError> {
    let encoder = device.command_encoder();
    let pass = encoder.begin_compute_pass();
    for (let round = 0; round < 10; round = round + 1) {
        GpuShaders.add_one.encode(device, pass, GpuDispatch.threads_1d(n), values, n);
    }
    pass.end();
    return device.submit(encoder.finish()).await;
}
```

Dispatches in one pass run in order, and each sees the writes of the ones before it. Mistakes made
while recording, such as using a buffer from another device, are returned by `submit`.

## Errors

Every GPU call returns `Result<..., GpuError>`, so `?` works inside an `async` function that returns
a `GpuError` result. Match on the variant to recover from a specific failure:

- `NoDevice`, `BackendNotCompiled` and `BackendUnavailable` come from `GpuDevice.create`.
- `DeviceLost` means the driver reset or the browser discarded the GPU. Every resource of that
  device is gone; create a new device and recreate them.
- `UnsupportedCapability` names the `GpuFeature` a pipeline needed.
- `ValidationFailed` carries the GPU API's own explanation of a rejected call.

`GpuError.message()` is a readable description, and `code()` is a stable code such as
`EDEVICELOST` for logs.

## Next steps

- [Writing shaders](shaders.md) covers what Dream code can do inside a shader.
- [Rendering](rendering.md) draws triangles offscreen and into a `system.window` window.
- [Ray tracing](ray-tracing.md) and [Machine learning](machine-learning.md) cover the advanced
  features.

<!-- pages -->
