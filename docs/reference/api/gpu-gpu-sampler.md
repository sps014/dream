# GpuSampler

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GpuSampler`

Sampling state for texture reads (`textureSample` / `textureSampleLevel`).

```dream
public class GpuSampler
```

## `id`

```dream
public get id(): int
```

## `linear`

Trilinear, clamp-to-edge.

```dream
public static fun linear(): GpuSampler
```

## `nearest`

Nearest-neighbor, clamp-to-edge.

```dream
public static fun nearest(): GpuSampler
```

## `create`

Builds a sampler from full state: filters, per-axis addressing, LOD clamp, anisotropy, and an optional depth comparison.

```dream
public static fun create(desc: GpuSamplerDesc): GpuSampler
```

## `destroy`

Releases the host sampler resource.

```dream
public fun destroy(): void
```
