# GpuRenderPipeline

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GpuRenderPipeline`

Render pipeline linking a `@vertex` + `@fragment` pair.

```dream
public class GpuRenderPipeline
```

## `id`

```dream
public get id(): int
```

## `depth_enabled`

```dream
public get depth_enabled(): bool
```

## `create`

Compiles and caches a render pipeline for the named Dream shaders. Available on web and native (`dream run` / wgpu); string-literal pairing is checked at compile time.

```dream
public static async fun create(vertex: string, fragment: string, token: Option<CancellationToken> = Option.None): Result<GpuRenderPipeline, GpuError>
```

## `create_ex`

Like `create`, with fixed-function state (topology, cull, depth, blend, MSAA).

```dream
public static async fun create_ex( vertex: string, fragment: string, desc: GpuRenderPipelineDesc, token: Option<CancellationToken> = Option.None ): Result<GpuRenderPipeline, GpuError>
```

## `destroy`

Releases the host pipeline resource.

```dream
public fun destroy(): void
```
