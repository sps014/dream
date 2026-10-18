---
title: Writing shaders
summary: Stage attributes, shader parameters, built-in functions and the parts of Dream that run on the GPU.
icon: material/function-variant
api: reference/api/system-gpu.md
---

# Writing shaders

A shader is a top-level Dream function with a stage attribute. Dream checks it like any other
function, then compiles it for the GPU when the program builds. Shader functions are not callable
from CPU code; run them through the generated `GpuShaders` class instead.

## Stages

| Attribute | Runs | Started by |
|---|---|---|
| `@compute(x, y, z)` | once per thread, in workgroups of `x * y * z` threads | `GpuShaders.<name>.dispatch` or `encode` |
| `@vertex` | once per vertex | a render pipeline's draw call |
| `@fragment` | once per covered pixel | a render pipeline's draw call |
| `@ray_generation`, `@closest_hit`, `@any_hit`, `@miss`, `@intersection`, `@callable` | per ray | a ray tracing pipeline, see [Ray tracing](ray-tracing.md) |

A compute workgroup's size is fixed when the program builds. Pick 64 or 256 threads for
one-dimensional work and 8 by 8 for images unless you have measured something better.

## Helper functions

Mark a function that shaders call with `@gpu`. A helper can call other helpers but not itself,
because GPUs have no call stack:

```dream
import system;
import system.gpu;

@gpu
fun luminance(color: GpuVec3): float {
    return color.dot(GpuVec3.of(0.2126f, 0.7152f, 0.0722f));
}

@compute(64, 1, 1)
fun grayscale(@readonly colors: GpuBuffer<GpuVec4>, out: GpuBuffer<float>, n: int): void {
    let i = global_id().x;
    if i < n {
        let c = colors[i];
        out[i] = luminance(GpuVec3.of(c.x, c.y, c.z));
    }
}
```

A `@gpu` helper exists only on the GPU, so CPU code cannot call it. When the CPU needs the same
calculation, write an ordinary function for it as well.

## Parameters

Each shader parameter becomes a parameter of its generated wrapper, in the same order:

| Parameter type | What the shader gets |
|---|---|
| `GpuBuffer<T>` | a buffer it can read and write; add `@readonly` when it only reads |
| `float`, `int`, `uint`, `bool`, `float16`, GPU vectors, `GpuMat4`, enums, structs of those | a value, the same for every thread |
| `GpuTexture` and `GpuSampler` | a texture to `sample` or `load`, and how to filter it |
| `GpuStorageTexture<T>` | an image it can read and write by pixel coordinate |
| `GpuAccelerationStructure` | a ray tracing scene, see [Ray tracing](ray-tracing.md) |
| `GpuHandle<T>` | a bindless texture or sampler, on devices with `GpuFeature.Bindless` |
| `GpuMlpWeights<T>` | a neural network layer, see [Machine learning](machine-learning.md) |

Vertex and fragment shaders also take stage inputs: the struct returned by the vertex shader, with
`@builtin("position")` and `@location(n)` fields, becomes the fragment shader's input. See
[Rendering](rendering.md).

## What Dream code can do in a shader

Shader bodies use a subset of Dream that maps directly onto GPU hardware:

- `let` bindings, assignments, arithmetic and comparisons on numbers, vectors and structs
- `if`/`else`, `while`, `break`, `continue`, `return`, and C-style `switch` with `case`/`default`
- `for (let i in 0..n)` over integer ranges, and C-style `for` loops
- structs and enums declared in your program, created with their constructors and passed by value
- `@gpu` helpers, `GpuMath`, vector methods such as `GpuVec3.normalize`, and the built-ins below

The generator rejects everything that needs a heap, a call stack or the operating system, and
reports each problem at its line in your Dream file:

- classes, unions, interfaces, strings, closures, `List`, `Map` and other library types
- recursion, `async`, `await` and generators
- pattern switch expressions, tuples, `?`, `is`, `sizeof` and labeled loops
- methods and operator overloads on your own structs, named arguments and explicit enum values
- generic shaders, and CPU functions such as `System.println`

When the Slang compiler itself rejects a shader, Dream maps its message back to the Dream line too.

## Built-in functions

| Function | Result |
|---|---|
| `global_id()` | the thread's position across the whole dispatch, a `GpuIVec3` |
| `local_id()`, `local_index()` | the thread's position inside its workgroup, as a vector or a flat index |
| `workgroup_id()`, `workgroup_count()` | the workgroup's position, and how many workgroups the dispatch has (not on Metal or WebGPU) |
| `vertex_id()`, `instance_id()` | the vertex and instance being drawn, in vertex shaders |
| `discard_fragment()` | drops the current pixel, in fragment shaders |
| `workgroup_barrier()`, `storage_barrier()` | waits until every thread of the workgroup reaches this point |

`GpuMath` has the usual scalar and vector math: `sin`, `cos`, `sqrt`, `pow`, `exp`, `log`, `min`,
`max`, `clamp`, `mix`, `step`, `smoothstep`, `reflect`, `fract`, and more. Each works on `float`,
`float16` and GPU vectors, component by component. `GpuMath` exists only inside shaders; on the
CPU use `Math`.

`GpuAtomic` updates a buffer element that many threads change at once: `add`, `add_float`, `sub`,
`min`, `max`, `and`, `or`, `xor`, `exchange` and `compare_exchange`.

## Workgroup memory

`workgroup_array<T>(n)` declares an array that all threads of one workgroup share. It is much
faster than a buffer, and it is the usual way to combine results inside a workgroup:

```dream
import system;
import system.gpu;

@compute(64, 1, 1)
fun group_sums(@readonly values: GpuBuffer<float>, sums: GpuBuffer<float>, n: int): void {
    let i = global_id().x;
    let partial = workgroup_array<float>(64);
    if i < n {
        partial[local_index()] = values[i];
    } else {
        partial[local_index()] = 0.0f;
    }
    workgroup_barrier();

    if local_index() == 0 {
        let total = 0.0f;
        for (let j in 0..64) {
            total = total + partial[j];
        }
        sums[workgroup_id().x] = total;
    }
}
```

The length must be a constant. Every thread of the workgroup must reach each
`workgroup_barrier()`, so never `return` early before a barrier; guard the work with `if`, as
above. WebGPU rejects a shader whose barrier depends on a per-thread condition.

## Subgroups

Threads also run in small hardware groups called subgroups, usually 32 or 64 wide.
`subgroup_add`, `subgroup_min`, `subgroup_max`, `subgroup_prefix_add`, `subgroup_ballot`,
`subgroup_broadcast`, `subgroup_shuffle` and the other `subgroup_` functions exchange values
within one without workgroup memory. Check `GpuFeature.Subgroups` first; browsers may not offer
them.

## float16

`float16` values and `GpuBuffer<float16>` halve memory use and are faster on many GPUs. Shaders that
use them need `GpuFeature.Float16`. Uploading and reading `float16` buffers works on every device.

## The generated GpuShaders class

Each module that contains shaders gets a `GpuShaders` class. It is `public` when any of the module's
shaders is public, and `internal` otherwise.

| Member | Use |
|---|---|
| `GpuShaders.backends()` | the backends this build has shader code for |
| `GpuShaders.<compute>.dispatch(device, dispatch, ...)` | runs a compute shader and waits for it |
| `GpuShaders.<compute>.encode(device, pass, dispatch, ...)` | records a dispatch into a `GpuComputePass` |
| `GpuShaders.<vertex>` and `GpuShaders.<fragment>` | stages to pass to `GpuDevice.render_pipeline` |
| `GpuShaders.<stage>.<parameter>` | the `GpuBindingSlot` to bind a render stage's parameter |
| `GpuShaders.<ray_generation>.trace(device, pipeline, width, height, ...)` | runs a ray tracing pipeline |

Wrapper parameters named `device`, `dispatch`, `pass`, `pipeline`, `width` or `height` get an
`_arg` suffix so they do not clash with the wrapper's own. A shader parameter named `descriptor`
gets the slot getter `descriptor_slot`.

`GpuDispatch` says how many threads to start:

- `GpuDispatch.threads_1d(n)`, `threads_2d(w, h)` and `threads_3d(x, y, z)` start enough
  workgroups to cover that many threads.
- `GpuDispatch.groups(x, y, z)` starts exactly that many workgroups.
