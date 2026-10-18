# GpuTexture

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuTexture`

An image in GPU memory that shaders sample and render passes draw into.  Create one with `GpuDevice.texture`. A window frame's `GpuFrame.target` is also a texture. Inside shaders, `sample`, `sample_level`, `sample_compare`, `load` and `size` read it.

```dream
public class GpuTexture
```

## `width`

Width in texels.

```dream
public get width(): int
```

## `height`

Height in texels.

```dream
public get height(): int
```

## `format`

How each texel is stored.

```dream
public get format(): GpuTextureFormat
```

## `write`

Replaces the texels of mip level `mip` with `data`, tightly packed rows from top to bottom in the texture's format.

```dream
public fun write(mip: int, data: byte[]): Result<bool, GpuError>
```

## `read`

Downloads mip level `mip` as tightly packed rows from top to bottom. It waits for submitted GPU work that writes the texture.

```dream
public async fun read(mip: int): Result<byte[], GpuError>
```

## `destroy`

Releases the GPU memory once submitted work that uses it has finished. Window frame targets belong to their surface and are not released.

```dream
public fun destroy(): void
```

## `sample`

Samples the texture at `uv`, where `(0, 0)` is the top-left corner and `(1, 1)` the bottom-right, filtered by `sampler`. The mip level follows the on-screen size, which only fragment shaders know.

```dream
public extern fun sample(sampler: GpuSampler, uv: GpuVec2): GpuVec4
```

## `sample_level`

Samples the texture at `uv` from mip level `level`; fractional levels blend two levels when the sampler filters mipmaps linearly.

```dream
public extern fun sample_level(sampler: GpuSampler, uv: GpuVec2, level: float): GpuVec4
```

## `sample_compare`

Compares `reference` with the depth texture at `uv` using the comparison sampler's `GpuCompare`, and returns the filtered fraction of texels that passed. Used for shadow maps.

```dream
public extern fun sample_compare(sampler: GpuSampler, uv: GpuVec2, reference: float): float
```

## `load`

Reads the texel at integer `coordinate` of mip level `level`, without filtering.

```dream
public extern fun load(coordinate: GpuIVec2, level: int): GpuVec4
```

## `size`

The size in texels of mip level `level`.

```dream
public extern fun size(level: int): GpuIVec2
```
