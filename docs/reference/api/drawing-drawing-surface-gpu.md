# DrawingSurface GPU integration

**Import:** `import system.drawing.gpu;`

Extends `DrawingSurface` with window presentation and GPU texture transfers. See the
[drawing guide](../../library/system/drawing.md#existing-gpu-windows) for setup and copy costs.

## `present`

Displays the rendered image through an existing GPU window surface. Dimensions must match. Caches texture and pipeline resources until close or a different target is used. Browser GPU rendering stays on the GPU; CPU and native rendering use a pixel upload.

```dream
public async fun present(surface: GpuSurface): Result<Unit, DrawingError>
```

## `copy_to`

Copies this surface into mip zero of an equally sized RGBA8 or BGRA8 GPU texture. Requires import system.drawing.gpu. Browser GPU rendering copies on the GPU; CPU and native rendering use a pixel upload. The target must be single-sampled.

```dream
public async fun copy_to(texture: GpuTexture): Result<Unit, DrawingError>
```
