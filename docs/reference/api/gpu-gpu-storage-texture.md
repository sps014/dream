# GpuStorageTexture

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuStorageTexture`

A 2D texture that shaders write texel by texel, such as the image a ray tracer renders.  Create one with `GpuDevice.storage_texture`. Inside a shader, `texture[coordinate]` reads and writes the texel at an integer pixel coordinate. On the CPU, `read` downloads every texel after the GPU finished.

```dream
public class GpuStorageTexture<T: unmanaged>
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

The storage format chosen from `T`.

```dream
public get format(): GpuTextureFormat
```

## `read`

Downloads every texel, row by row from the top. It waits for submitted GPU work that writes the texture.

```dream
public async fun read(): Result<T[], GpuError>
```

## `destroy`

Releases the GPU memory once submitted work that uses it has finished.

```dream
public fun destroy(): void
```

## `this`

Reads the texel at `coordinate` inside a shader.

```dream
public extern fun this[coordinate: GpuIVec2]: T
```

## `this`

Writes the texel at `coordinate` inside a shader. Writes outside the texture are dropped.

```dream
public extern fun this[coordinate: GpuIVec2] = value: T
```
