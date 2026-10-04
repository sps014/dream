# GpuTexture

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GpuTexture`

A GPU texture of any format and shape the adapter supports. `create` takes a `GpuTextureDesc`; the named constructors below are the common shapes spelled out. Allocation can fail — a block-compressed format on an adapter without that feature, or an impossible shape like a multisampled mip chain — so `create` returns a `Result`.

```dream
public class GpuTexture
```

## `width: int`

```dream
public width: int
```

## `height: int`

```dream
public height: int
```

## `depth_or_layers: int`

Slices for `D3`, array layers otherwise. `6` for a cubemap.

```dream
public depth_or_layers: int
```

## `mip_levels: int`

```dream
public mip_levels: int
```

## `sample_count: int`

```dream
public sample_count: int
```

## `format: GpuTextureFormat`

```dream
public format: GpuTextureFormat
```

## `view_dimension: GpuTextureViewDimension`

```dream
public view_dimension: GpuTextureViewDimension
```

## `id`

```dream
public get id(): int
```

## `create`

Allocates a texture matching `desc`.

```dream
public static fun create(desc: GpuTextureDesc): Result<GpuTexture, GpuError>
```

## `rgba8`

An empty RGBA8 texture of `width`×`height`.

```dream
public static fun rgba8(width: int, height: int): GpuTexture
```

## `from_image_bytes`

Decodes PNG or JPEG bytes into a new RGBA8 2D texture. The alpha channel is kept straight (not premultiplied). Fails with `GpuError.unsupported` for an unknown codec or an image larger than 8192 on an edge, and `GpuError.validation` for empty input.

```dream
public static async fun from_image_bytes( data: byte[], token: Option<CancellationToken> = Option.None ): Result<GpuTexture, GpuError>
```

## `depth24`

Depth attachment texture (`depth24plus`) for mesh pipelines.

```dream
public static fun depth24(width: int, height: int): GpuTexture
```

## `rgba16float`

HDR color texture (`rgba16float`) for post / bloom targets.

```dream
public static fun rgba16float(width: int, height: int): GpuTexture
```

## `cube_rgba8`

Cube map with six `size`×`size` RGBA8 faces.

```dream
public static fun cube_rgba8(size: int): GpuTexture
```

## `write_rgba`

Uploads a full-texture RGBA8 pixel buffer.

```dream
public async fun write_rgba(pixels: byte[], token: Option<CancellationToken> = Option.None): Result<bool, GpuError>
```

## `write_rgba_at`

Uploads an RGBA8 sub-rectangle at (`x`, `y`) of size `w`×`h`.

```dream
public async fun write_rgba_at( x: int, y: int, w: int, h: int, pixels: byte[], token: Option<CancellationToken> = Option.None ): Result<bool, GpuError>
```

## `read_rgba`

Downloads the full texture as RGBA8 bytes.

```dream
public async fun read_rgba(token: Option<CancellationToken> = Option.None): byte[]
```

## `copy_from_buffer`

GPU-side copy from a tightly packed RGBA8 buffer (bytes = width×height×4 for a full copy).

```dream
public fun copy_from_buffer(src: GpuBuffer<byte>, byte_offset: int): void
```

## `copy_to_buffer`

GPU-side copy of the full texture into a byte buffer (must hold width×height×4 bytes).

```dream
public fun copy_to_buffer(dst: GpuBuffer<byte>, byte_offset: int): void
```

## `copy_from`

GPU-side copy from `src` into this texture at `(dst_x, dst_y)`.

```dream
public fun copy_from(src: GpuTexture, src_x: int, src_y: int, dst_x: int, dst_y: int, w: int, h: int): void
```

## `generate_mipmaps`

Generates mip levels for this texture (rgba8); returns `Err` when unsupported.

```dream
public fun generate_mipmaps(): Result<bool, GpuError>
```

## `destroy`

Releases the host texture resource.

```dream
public fun destroy(): void
```
