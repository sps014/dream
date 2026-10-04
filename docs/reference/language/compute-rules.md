# Write a compute function

This guide describes accepted function parameters, workgroup settings, and the operations available inside a compute function.

[Back to overview](compute.md)

## Attribute

| Form | Meaning |
|------|---------|
| `@compute` | Workgroup size `(64, 1, 1)` |
| `@compute(x)` | `(x, 1, 1)` |
| `@compute(x, y)` | `(x, y, 1)` |
| `@compute(x, y, z)` | Full 3D workgroup |

Only **top-level** `fun`s may carry `@compute`.
Kernels must return `void`, cannot be `async`/`extern`/generic, and are **not** callable as CPU functions — use `Compute.run_1d` / `Compute.run_2d` with the kernel **name**.


## Storage parameters

Kernel storage buffers are **`GpuBuffer<T>`** (not bare `T[]`).
Inside a kernel you can index them (`a[i]`) and read **`a.length`**.

- Scalars and unmanaged value structs become uniforms, laid out in declaration order — `Uniforms.pack_i32` / `pack_f32` must match that layout.
- `Compute.run_1d` / `run_2d` pack their extents as the uniform blob, so a kernel whose only uniform is a bound (`n: int`) gets the grid size for free; use `run_3d` to supply uniforms yourself.
- The grid is also always readable in-kernel through `num_workgroups`.

Prefix a buffer with **`@readonly`** for a read-only storage buffer instead of read/write:

```dream
@compute(64)
fun scale(@readonly a: GpuBuffer<float>, out: GpuBuffer<float>, n: int): void {
    let i = global_id.x;
    if i < n { out[i] = a[i] * 2.0; }
}
```

A `GpuTexture` parameter is a **sampled** `texture_2d<f32>` by default:

- **`@storage`** — writable `texture_storage_2d<rgba8unorm, write>` (what `Gpu.texture_store` needs)
- **`@cube`** — `texture_cube<f32>`

Bindings land in `@group(0)` with auto-assigned indices; **`@group(N)`** / **`@binding(N)`** name them explicitly.
See [Shaders → Resource bindings](shader-resources.md#resource-bindings).

Host dispatch still passes `GpuBuffer` instances to `Compute.run_*` in binding order.
Kernels may also take `GpuTexture` / `GpuSampler`; use `Compute.run_resources` or `ComputePass.dispatch_resources` to supply their host ids.


## Builtins

Inside a kernel, these locals are in scope (typed as `GpuId3` with `.x`/`.y`/`.z`):

- `global_id` — global invocation id
- `local_id` — local invocation id
- `workgroup_id` — workgroup id
- `num_workgroups` — dispatch size in workgroups
- `local_invocation_index` — `local_id` flattened to a single `int`, the natural index into workgroup memory


## Language surface

Allowed:

- `if`/`else`, `while`/`do`/`for`, `break`/`continue`, early `return`, ternary
- Integer `switch` (including simple enum members)
- Arithmetic/bitwise (shifts count as arithmetic; the right operand is unsigned)
- `GpuMath.pack*` / `unpack*`, `GpuBuffer` indexing / `.length`
- Unmanaged value structs
- Calls to **`@gpu` helpers** (and other `@compute` kernels)
- `Gpu.workgroup_barrier` / `Gpu.storage_barrier`, `Gpu.atomic_*`, `Gpu.texture_*`, `GpuMath.*`

`Gpu.atomic_compare_exchange(buf, i, cmp, v)` stores `v` only if `buf[i]` holds `cmp`, and reports the value that was there beforehand — so the store happened exactly when the result equals `cmp`.
It may also fail spuriously, which is why it belongs in a retry loop.

Texture reads from a kernel need an explicit mip level:

- `Gpu.texture_load` — unfiltered texel fetch
- `Gpu.texture_sample_level` — filtered at a level you name

See [sampling textures](shader-resources.md#sampling-textures) for the full set.

A `switch` evaluates its subject once, and its case labels must be constants.
Cases do not fall through, so a `break` in a case body leaves the enclosing loop rather than the `switch`.

Forbidden:

- Labelled `break`/`continue` (always apply to the innermost loop — use a flag local or move the inner loop into a `@gpu` helper and `return`)
- Bare `T[]` as a kernel param
- `string` / `List` / `class` / `js` / `async`
- `for..in`
- Union pattern-match `switch`
- `lock`
- Recursion
- Calling ordinary CPU functions that are **not** marked `@gpu`
- Calling `@gpu` / `@compute` / `@vertex` / `@fragment` from normal CPU code (stages dispatch via `Compute.run` / `GpuRenderPipeline.create`)

`sizeof(T)` is allowed and becomes an integer literal (see [`sizeof` / `nameof` / `typeof`](type-queries.md#sizeof-nameof-and-typeof)).
`nameof(...)` and `typeof(...)` are not — they yield `string`, which is forbidden on the GPU.

See [`@gpu` helpers](shader-functions.md#gpu-helpers).

### Workgroup memory

```dream
@compute(64)
fun reduce(data: GpuBuffer<float>, out: GpuBuffer<float>): void {
    @workgroup(64) let tile: float;
    let lid = local_id.x;
    tile[lid] = data[global_id.x];
    Gpu.workgroup_barrier();
    // …
}
```

`@workgroup(N) let name: T;` declares workgroup-shared scratch of length `N` of type `T`.

### `shared` is not GPU shared memory

Dream's `shared` class modifier marks **CPU / `Task`** classes that can be shared across threads.
It is illegal inside `@compute`.
GPU scratch uses `@workgroup`, not `shared`.
