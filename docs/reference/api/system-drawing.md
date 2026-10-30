# system.drawing

Import `system.drawing` to record and render 2D canvases on CPU or GPU.
Read the [drawing guide](../../library/system/drawing.md) for examples, backdrop behavior,
pixel formats, browser setup and current feature coverage.

| API | Purpose |
| --- | --- |
| [Canvas](drawing-canvas.md) | Record shapes, clips, transforms, opacity layers and backdrop panels |
| [DrawingSurface](drawing-drawing-surface.md) | Render offscreen and read pixels |
| [DrawingBackend](drawing-drawing-backend.md) | Choose automatic, CPU or GPU rendering |
| [Backdrop](drawing-backdrop.md) | Blur earlier canvas content and apply panel tint |
| [DrawingPath](drawing-drawing-path.md) | Build reusable vector geometry |
| [Paint](drawing-paint.md) | Supply a solid color or gradient |
| [Color](drawing-color.md) | Describe sRGB colors and opacity |
| [Rect](drawing-rect.md) | Position and size rectangles |
| [Picture](drawing-picture.md) | Reuse immutable recorded drawing |
| [DrawingError](drawing-drawing-error.md) | Inspect drawing failures |
| [Image](drawing-image.md) | Decode PNG/JPEG or create RGBA images |
| [TextStyle](drawing-text-style.md) | Set font, color, wrapping and alignment |
| [TextLayout](drawing-text-layout.md) | Measure and reuse shaped text |
| [TextAlignment](drawing-text-alignment.md) | Align paragraphs |
| [Fonts](drawing-fonts.md) | Register font bytes |

| [StrokeStyle](drawing-stroke-style.md) | Set stroke width, caps, joins and dashes |
| [StrokeCap](drawing-stroke-cap.md) | Choose open stroke endpoints |
| [StrokeJoin](drawing-stroke-join.md) | Choose stroke corners |
| [GradientStop](drawing-gradient-stop.md) | Position a color in a gradient |
| [Effects](drawing-effects.md) | Apply ordered blur, shadow and color adjustments |
| [BlendMode](drawing-blend-mode.md) | Composite an isolated group |

Import `system.drawing.gpu` for [`DrawingSurface.present` and `copy_to`](drawing-drawing-surface-gpu.md): display through an existing GPU window surface or upload to a texture.
