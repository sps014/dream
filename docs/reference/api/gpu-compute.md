# Compute

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Compute`

```dream
public static class Compute
```

## `run_1d`

Dispatches `kernel` over a 1D grid of size `n` (float storage buffers only). The extent is packed as the uniform blob, so a kernel whose only uniform is a bound (`fun k(..., n: int)`) receives the grid size without packing anything. Kernels needing more uniforms than the extents should use `run_3d`.

```dream
public static async fun run_1d( kernel: string, buffers: GpuBuffer<float>[], n: int, token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `run_2d`

Dispatches `kernel` over a 2D grid of size `w`×`h` (float storage buffers only). Like `run_1d`, the extents are packed as the uniform blob.

```dream
public static async fun run_2d( kernel: string, buffers: GpuBuffer<float>[], w: int, h: int, token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `run_2d_uniforms`

Like `run_2d`, with a little-endian uniform blob.

```dream
public static async fun run_2d_uniforms( kernel: string, buffers: GpuBuffer<float>[], w: int, h: int, uniforms: byte[], token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `run_3d`

Dispatches `kernel` over a 3D grid with optional uniforms (float storage buffers only).

```dream
public static async fun run_3d( kernel: string, buffers: GpuBuffer<float>[], sx: int, sy: int, sz: int, uniforms: byte[], token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `run`

Dispatches with a typed `GpuBindList` (mixed buffers / textures / samplers).

```dream
public static async fun run( kernel: string, resources: GpuBindList, sx: int, sy: int, sz: int, uniforms: byte[], token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `run_resources`

```dream
public static async fun run_resources( kernel: string, buffer_ids: int[], texture_ids: int[], sampler_ids: int[], sx: int, sy: int, sz: int, uniforms: byte[], token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `dispatch_indirect`

Indirect dispatch: `args` is a GPU buffer of three u32 workgroup counts at byte offset 0.

```dream
public static async fun dispatch_indirect( kernel: string, buffers: GpuBuffer<float>[], args: GpuBuffer<int>, token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `dispatch_indirect_bind`

Indirect dispatch with a typed `GpuBindList`.

```dream
public static async fun dispatch_indirect_bind( kernel: string, resources: GpuBindList, args: GpuBuffer<int>, token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `run_shader`

Dispatches a raw `GpuShader` over a 3D workgroup grid.

```dream
public static async fun run_shader( shader: GpuShader, buffers: GpuBuffer<float>[], wx: int, wy: int, wz: int, token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```
