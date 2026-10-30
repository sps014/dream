# Drawing Surface

**Import:** `import system.drawing;`

## `DrawingSurface`

A persistent CPU or GPU offscreen drawing target. Pixel reads synchronize GPU work explicitly.

```dream
public class DrawingSurface
```

## `create`

Creates a surface of 1..8192 pixels per side and at most 16777216 pixels. Auto prefers GPU and falls back to CPU during initialization.

```dream
public static async fun create(
    width: int,
    height: int,
    backend: DrawingBackend = DrawingBackend.Auto
): Result<DrawingSurface, DrawingError>
```

## `backend`

The selected rendering backend; Auto is resolved to Cpu or Gpu.

```dream
public get backend(): DrawingBackend
```

## `render`

Submits the current canvas, replacing the previous image. Canvas and surface dimensions must match; pixel reads wait for GPU completion.

```dream
public fun render(borrow canvas: Canvas): Result<Unit, DrawingError>
```

## `render`

Submits a reusable picture, replacing the previous image. Picture and surface dimensions must match; pixel reads wait for GPU completion.

```dream
public fun render(borrow picture: Picture): Result<Unit, DrawingError>
```

## `read_pixels`

Reads tightly packed premultiplied sRGB RGBA8 pixels, top row first. GPU reads wait for completion.

```dream
public async fun read_pixels(): Result<byte[], DrawingError>
```

## `close`

Releases this surface. Subsequent rendering and reads return a DrawingError.

```dream
public fun close(): void
```
