# GpuDevice

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuDevice`

A connection to one GPU through one graphics API. Every buffer, texture, pipeline and surface belongs to the device that created it, and using it with another device returns `GpuError.WrongDevice`.  Create one with `GpuDevice.create`. Releasing the last reference to the device releases it on the host once the GPU finished its submitted work.

```dream
public class GpuDevice
```

## `create`

Opens a device on `backend`.  `GpuBackend.Auto` picks the platform's native API when this build has shader code for it, and WebGPU otherwise. Any other backend is used exactly as asked: when this build has no shader code for it the result is `GpuError.BackendNotCompiled`, and when the machine cannot run it the result is `GpuError.BackendUnavailable`. A program without shaders may open any backend.

```dream
public static async fun create(backend: GpuBackend): Result<GpuDevice, GpuError>
```

## `backend`

The API this device runs on; never `GpuBackend.Auto`.

```dream
public get backend(): GpuBackend
```

## `adapter_name`

The driver's name for this device's graphics adapter, or an empty string when
unavailable. Browsers may hide identifying information for privacy. This is a
display label; use `supports` to decide which GPU features can run.

```dream
public get adapter_name(): string
```

## `supports`

True when the device offers `feature`.

```dream
public fun supports(feature: GpuFeature): bool
```

## `check`

`Ok(true)` while the device works, or `GpuError.DeviceLost` once the driver or browser took it away. Calls that touch a lost device also fail with `DeviceLost`.

```dream
public fun check(): Result<bool, GpuError>
```

## `buffer_from`

Creates a storage buffer that shaders can read and write, filled with `data`.

```dream
public fun buffer_from<T: unmanaged>(data: T[]): Result<GpuBuffer<T>, GpuError>
```

## `buffer_alloc`

Creates a storage buffer of `length` elements, all zero.

```dream
public fun buffer_alloc<T: unmanaged>(length: int): Result<GpuBuffer<T>, GpuError>
```

## `vertex_buffer_from`

Creates a vertex buffer filled with `data`, one element per vertex. Describe its layout to the pipeline with `GpuVertexLayout` and bind it with `GpuRenderPass.set_vertex_buffer`.

```dream
public fun vertex_buffer_from<T: unmanaged>(data: T[]): Result<GpuBuffer<T>, GpuError>
```

## `index_buffer_from`

Creates an index buffer of 32-bit indexes for `GpuRenderPass.draw_indexed`.

```dream
public fun index_buffer_from(data: uint[]): Result<GpuBuffer<uint>, GpuError>
```

## `index_buffer_from`

Creates an index buffer of 16-bit indexes, which halves its memory when meshes have fewer than 65,536 vertices.

```dream
public fun index_buffer_from(data: ushort[]): Result<GpuBuffer<ushort>, GpuError>
```

## `texture`

Creates a texture described by `desc`, with every texel zero.

```dream
public fun texture(desc: GpuTextureDesc): Result<GpuTexture, GpuError>
```

## `storage_texture`

Creates a `width` by `height` texture that shaders write texel by texel.  `T` chooses the format: `GpuVec4` stores four floats, `GpuVec2` two, `float` one and `uint` one unsigned integer. Other element types return `GpuError.InvalidResource`.

```dream
public fun storage_texture<T: unmanaged>(
    width: int,
    height: int
): Result<GpuStorageTexture<T>, GpuError>
```

## `sampler`

Creates a sampler, which tells shaders how to filter and wrap texture reads.

```dream
public fun sampler(desc: GpuSamplerDesc): Result<GpuSampler, GpuError>
```

## `handle`

A bindless handle to `texture`, which can be stored in any buffer and sampled by shaders. It stays valid while the texture is alive. Needs `GpuFeature.Bindless`.

```dream
public fun handle(texture: GpuTexture): Result<GpuHandle<GpuTexture>, GpuError>
```

## `handle`

A bindless handle to `sampler`. Needs `GpuFeature.Bindless`.

```dream
public fun handle(sampler: GpuSampler): Result<GpuHandle<GpuSampler>, GpuError>
```

## `create_surface`

Connects the device to `window` so frames can be drawn into it. The surface follows the window's vsync hint from `WindowDesc.vsync`. Headless and closed windows return `GpuError.SurfaceLost`.

```dream
public fun create_surface(window: Window): Result<GpuSurface, GpuError>
```

## `render_pipeline`

Creates a render pipeline from a `@vertex` and a `@fragment` shader, both given as their `GpuShaders` descriptors. Every `@location` the fragment shader reads must be written by the vertex shader; otherwise the result is `GpuError.Validation`.

```dream
public fun render_pipeline(
    vertex_shader: GpuStageShader,
    fragment_shader: GpuStageShader,
    desc: GpuRenderPipelineDesc
): Result<GpuRenderPipeline, GpuError>
```

## `ray_tracing_pipeline`

Creates a ray tracing pipeline from a `@ray_generation` shader, the `@miss` shaders `trace_ray` selects by index, the hit groups instances select by their hit group offset, and the `@callable` shaders `call_shader` selects by index. Needs `GpuFeature.RayTracing`.

```dream
public fun ray_tracing_pipeline(
    ray_generation: GpuStageShader,
    misses: GpuStageShader[],
    hit_groups: GpuHitGroup[],
    callables: GpuStageShader[]
): Result<GpuRayTracingPipeline, GpuError>
```

## `build_bottom_level`

Builds a bottom-level acceleration structure, the ray tracing index of one mesh. Needs `GpuFeature.RayTracing` or `GpuFeature.RayQuery`.

```dream
public async fun build_bottom_level(
    geometry: GpuGeometry
): Result<GpuAccelerationStructure, GpuError>
```

## `build_top_level`

Builds a top-level acceleration structure from mesh instances. Shaders trace rays against it. Needs `GpuFeature.RayTracing` or `GpuFeature.RayQuery`.

```dream
public async fun build_top_level(
    instances: GpuInstance[]
): Result<GpuAccelerationStructure, GpuError>
```

## `upload_mlp`

Uploads one neural network layer for `GpuMlpWeights.apply`: `weights` holds `outputs` rows of `inputs` values each, and `biases` one value per output. On Vulkan and D3D12 devices with `GpuFeature.CooperativeVectors` the host converts the weights into the device's own matrix layout; elsewhere it keeps them row by row. Uploading works on any device. `T` is `float16` or `float`; other element sizes fail with `GpuError.InvalidResource`.

```dream
public fun upload_mlp<T: unmanaged>(
    weights: T[],
    biases: T[],
    inputs: int,
    outputs: int
): Result<GpuMlpWeights<T>, GpuError>
```

## `command_encoder`

Starts recording GPU commands. Submit them with `submit`.

```dream
public fun command_encoder(): GpuCommandEncoder
```

## `submit`

Runs recorded commands and waits until the GPU finished them. Recording errors, such as a resource from another device, are reported here.

```dream
public async fun submit(commands: GpuCommandBuffer): Result<bool, GpuError>
```
