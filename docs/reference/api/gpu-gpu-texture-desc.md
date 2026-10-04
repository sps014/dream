# GpuTextureDesc

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct GpuTextureDesc`

Shape, format, and intended use of a texture, for `GpuTexture.create`. Build one with `GpuTextureDesc.d2(...)` (or `d2_array` / `d3` / `cube`) and then adjust the optional fields:  ```dream let d = GpuTextureDesc.d2(1024, 1024, GpuTextureFormat.Rgba8Unorm); d.mip_levels = 11; let albedo = GpuTexture.create(d)?; ```

```dream
public struct GpuTextureDesc
```

## `format: GpuTextureFormat`

Texel format. Depth formats produce a depth attachment, not a writable color texture.

```dream
public format: GpuTextureFormat
```

## `dimension: GpuTextureDimension`

Physical grid shape. `D3` uses `depth_or_layers` as a depth, everything else as a layer count.

```dream
public dimension: GpuTextureDimension
```

## `view_dimension: GpuTextureViewDimension`

How shader bindings view the layers. Six layers viewed as `Cube` are a cubemap; the same six viewed as `D2Array` are an array — the host cannot guess, so this is explicit.

```dream
public view_dimension: GpuTextureViewDimension
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

Array layers for `D1`/`D2`, or slices for `D3`. Cube views need a multiple of 6.

```dream
public depth_or_layers: int
```

## `mip_levels: int`

Mip chain length. `1` means no mips; a larger value allocates the levels but leaves them undefined until `generate_mipmaps` or an explicit per-level write fills them.

```dream
public mip_levels: int
```

## `sample_count: int`

MSAA samples. Must be `1` unless the texture is used only as a render attachment, and multisampled textures cannot have mips, be arrays, or be written from the CPU.

```dream
public sample_count: int
```

## `storage_access: Option<GpuStorageAccess>`

Non-`None` marks the texture usable as a WGSL `texture_storage_*` binding with this access. Only formats where `GpuTextureFormat` permits storage can be used.

```dream
public storage_access: Option<GpuStorageAccess>
```

## `d2`

A plain 2D texture.

```dream
public static fun d2(width: int, height: int, format: GpuTextureFormat): GpuTextureDesc
```

## `d2_array`

A 2D array of `layers` slices, bound as `texture_2d_array`.

```dream
public static fun d2_array(width: int, height: int, layers: int, format: GpuTextureFormat): GpuTextureDesc
```

## `d3`

A volume texture, bound as `texture_3d`.

```dream
public static fun d3(width: int, height: int, depth: int, format: GpuTextureFormat): GpuTextureDesc
```

## `cube`

A cubemap with six `size`×`size` faces, bound as `texture_cube`.

```dream
public static fun cube(size: int, format: GpuTextureFormat): GpuTextureDesc
```

## `cube_array`

A cubemap array of `cubes` cubemaps, bound as `texture_cube_array`.

```dream
public static fun cube_array(size: int, cubes: int, format: GpuTextureFormat): GpuTextureDesc
```
