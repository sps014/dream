# GpuCommandEncoder

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuCommandEncoder`

Records GPU work, such as render passes, compute passes and copies, to submit together.  Get one from `GpuDevice.command_encoder` or a window frame's `GpuFrame.encoder`. Passes run in the order they were begun; end each pass before beginning the next. Mistakes such as a resource from another device do not fail immediately: the first one is returned by `GpuDevice.submit`.

```dream
public class GpuCommandEncoder
```

## `begin_render_pass`

Begins drawing into the targets of `desc`. End the pass with `GpuRenderPass.end`.

```dream
public fun begin_render_pass(desc: GpuRenderPassDesc): GpuRenderPass
```

## `begin_compute_pass`

Begins a compute pass. Record dispatches into it with the generated `GpuShaders.<name>.encode`, then end it with `GpuComputePass.end`.

```dream
public fun begin_compute_pass(): GpuComputePass
```

## `copy_buffer`

Copies `count` elements from `source`, starting at element `source_offset`, into `destination` starting at `destination_offset`. Both ranges must lie inside their buffers. Record copies outside passes.

```dream
public fun copy_buffer<T: unmanaged>(
    source: GpuBuffer<T>,
    source_offset: int,
    destination: GpuBuffer<T>,
    destination_offset: int,
    count: int
): void
```

## `finish`

Ends recording. Pass the result to `GpuDevice.submit`.

```dream
public fun finish(): GpuCommandBuffer
```
