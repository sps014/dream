# GpuBindGroup

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GpuBindGroup`

A bind group built once against a pipeline's `@group(N)` layout and reused across frames. This is what makes a material system cheap: the per-material texture/sampler set is resolved into a host object at load time instead of being rebuilt on every draw. ```dream let material = (GpuBindGroup.create( pipe, 1, GpuBindList.begin().texture(albedo).sampler(linear) ).await)?; pass.set_bind_group(1, material); ```  The uniform block is *not* part of a bind group: it is supplied per draw with `GpuRenderPassEncoder.set_uniforms`, so the host combines the two when it builds the group.

```dream
public class GpuBindGroup
```

## `id`

```dream
public get id(): int
```

## `group`

Which `@group(N)` this was built for.

```dream
public get group(): int
```

## `create`

Resolves `binds` against `pipeline`'s layout for `group`. Resources are consumed in the order the shader declares them within that group, separately per kind.

```dream
public static async fun create( pipeline: GpuRenderPipeline, group: int, binds: GpuBindList, token: Option<CancellationToken> = Option.None ): Result<GpuBindGroup, GpuError>
```

## `destroy`

Releases the host bind group.

```dream
public fun destroy(): void
```
