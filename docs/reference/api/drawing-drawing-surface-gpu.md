# DrawingSurface GPU integration

**Import:** `import system.drawing.gpu;`

Extends `DrawingSurface` with window presentation and texture uploads. See the
[drawing guide](../../library/system/drawing.md#existing-gpu-windows) for setup and copy costs.

## `present`

Displays the rendered image through an existing GPU window surface. Dimensions must match. Caches upload and pipeline resources until close or a different target is used; GPU drawing first waits for readback.

```dream
public async fun present(surface: GpuSurface): Result<Unit, DrawingError>
```

## `copy_to`

Copies this surface into mip zero of an equally sized RGBA8 or BGRA8 GPU texture. Requires import system.drawing.gpu; waits for readback when this surface uses GPU rendering.

```dream
public async fun copy_to(texture: GpuTexture): Result<Unit, DrawingError>
```
