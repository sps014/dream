---
title: Drawing
summary: Draw shapes, images, text and backdrop panels on CPU or GPU.
---

# Drawing

Import `system.drawing` to draw into an offscreen image. Create a canvas, record shapes, then render
it into a surface. The same canvas works with the CPU and GPU backends.

```dream
import system;
import system.drawing;

async fun draw_panel(): Result<byte[], DrawingError> {
    let canvas = Canvas(800, 600);
    canvas.clear(Color.rgb(30, 40, 60));
    canvas.fill_circle(220.0f, 180.0f, 100.0f, Color.rgb(80, 160, 240));
    canvas.backdrop(
        Rect(40.0f, 40.0f, 300.0f, 200.0f),
        Backdrop.blur(20.0f).tint(Color.rgba(255, 255, 255, 0.15f)).rounded(16.0f)
    );

    let surface = DrawingSurface.create(800, 600).await?;
    surface.render(canvas)?;
    return surface.read_pixels().await;
}

async fun main(): void {
    switch (draw_panel().await) {
        Ok(pixels) => {
            System.println("Rendered " + pixels.length.to_string() + " bytes");
        }
        Err(error) => {
            System.println(error.message);
        }
    }
}
```

`DrawingBackend.Auto`, the default, attempts GPU initialization and falls back to CPU. Pass
`DrawingBackend.Cpu` to render without a GPU, or `DrawingBackend.Gpu` to require one. The surface's
`backend` property reports the selected backend. A surface permits dimensions from 1 to 8192 pixels
per side, with at most 16777216 pixels in total. Canvas dimensions must match the surface.

Coordinates start at the top-left corner, with positive y downward. RGB color channels are integers
from 0 to 255; alpha is a float from 0 to 1. `read_pixels` returns tightly packed premultiplied sRGB
RGBA8 bytes, top row first. Reading GPU pixels waits for submitted work. Each render replaces the
previous image, including transparent regions.

## Backdrop panels

`canvas.backdrop(rect, effect)` filters earlier drawing in its current isolated group, or the
canvas root when no group is open. Parent groups and later siblings are excluded. The blur can sample
outside the panel, but the result and tint are clipped to the panel and the current clip. Drawing
recorded afterward stays sharp. A zero blur sigma applies only the tint. Sigma is measured in physical
pixels, ranges from 0 to 128, and does not scale with the current transform.

The effect sees only this canvas. It cannot sample the desktop or other application windows. A
picture permits at most eight backdrop panels across all groups. An enclosing group applies its
filters, opacity and blending after its nested panels have been composed.

Add `.noise(opacity, seed: 0)` for acrylic-style grain after blur and tint. Opacity is 0..1;
noise preserves the filtered panel's alpha. A seed selects a deterministic 64 by 64 monochrome
tile shared by CPU and GPU rendering. The tile is anchored in device pixels so repeated renders
and changing transforms do not animate the grain. Later edits to a `Backdrop` do not change
panels already recorded.

```dream
canvas.backdrop(
    Rect(40, 40, 320, 200),
    Backdrop.blur(20).tint(Color.rgba(255, 255, 255, 0.15f)).noise(0.04f).rounded(16)
);
```

## Paths, gradients and clipping

Use `DrawingPath` for reusable shapes. A draw captures the path and paint at the time of the call.
Changing them afterward does not change earlier drawing.

```dream
let canvas = Canvas(320, 240);
let panel = DrawingPath.rect(Rect(20.0f, 20.0f, 280.0f, 200.0f), 12.0f);
let paint = Paint.linear(
    20.0f,
    20.0f,
    300.0f,
    220.0f,
    [GradientStop(0, Color.rgb(40, 100, 200)), GradientStop(1, Color.rgb(140, 60, 180))]
);
canvas.fill(panel, paint);
canvas.save();
canvas.clip(panel);
canvas.translate(10.0f, 0.0f);
canvas.fill_circle(160.0f, 120.0f, 60.0f, Color.white);
canvas.restore();
```

Pair every `save` or `save_layer` with `restore`. `save_layer(opacity)` composites its contents as
one isolated group, so overlapping shapes do not each apply the group opacity. Save depth and clip
depth are limited to 64. `finish()` captures an immutable `Picture` for reuse; rendering a canvas
captures one automatically. `clear` discards the previous recording and resets canvas state.

`finish` rejects unbalanced state. Rendering additionally validates colors, paths, transforms, blur
sigma and command limits. Invalid input returns `DrawingError`. Geometry values must be finite and
within +/-1000000; a picture permits at most 4000000 encoded float values.

## Stroke styles and blending

Pass `StrokeStyle(width)` and a `Paint` to `canvas.stroke`. The style's `cap`, `join`,
`miter_limit`, `dashes` and `dash_offset` are captured when recorded. Dash lengths must be positive;
an empty list draws a continuous stroke. Gradient paints accept 2..256 stops sorted within 0..1.
Equal offsets produce hard transitions. `Paint.sweep` measures clockwise angles in radians.

```dream
let outline = StrokeStyle(2);
outline.cap = StrokeCap.Round;
outline.join = StrokeJoin.Round;
outline.dashes = [6.0f, 3.0f];
canvas.stroke(panel, paint, outline);
canvas.save_layer(opacity: 0.8f, blend: BlendMode.Screen);
canvas.fill_circle(160, 120, 60, Color.rgb(80, 120, 220));
canvas.restore();
```

Blending applies to the whole isolated group after group opacity, using earlier parent content
as its destination. Choose SourceOver, Multiply, Screen, Overlay, Darken, Lighten, Difference or
Exclusion. Invalid styles, colors and stop arrays return `DrawingError` at render.

## Group filters and shadows

An `Effects()` chain runs in call order on an isolated group. Pair `save_layer` with `restore`.
Effects run before group opacity and parent blending; the active clip bounds their output.

```dream
let effects = Effects().shadow(0, 8, 16, Color.rgba(0, 0, 0, 0.25f)).saturation(0.8f);
canvas.save_layer(effects: Option.Some(effects));
canvas.fill_rect(40, 40, 240, 120, Color.white, 16);
canvas.restore();
```

`blur(sigma)` and `shadow(dx, dy, sigma, color)` use physical pixels, independent of transforms.
Sigma is 0..128; shadow offsets are within +/-8192. Shadows include the original input.
`brightness`, `contrast` and `saturation` accept factors in 0..8; one preserves the input.
Brightness multiplies straight sRGB channels; contrast scales their distance from 0.5;
saturation scales their distance from luminance. Channels are clamped after every operation,
then premultiplied again. Color operations preserve alpha and keep transparent pixels colorless.

Recording captures the chain, so later changes do not affect earlier groups. A chain permits at
most 16 operations. Invalid values and exceeding 256 MiB of pooled full-surface scratch targets
return `DrawingError` at render. GPU filters stay on the GPU. `read_pixels` explicitly synchronizes and downloads pixels;
native GPU presentation reads pixels through its upload bridge unless a Windows destination uses D3D12 or Vulkan
on the same adapter with shared-resource support. Inset shadows, spread and arbitrary filter
graphs are not provided.

## Images and text

Decode PNG or JPEG bytes with `Image.decode(bytes).await`, or create an image from straight RGBA8
bytes with `Image.from_rgba(width, height, bytes).await`. Both return `Result<Image, DrawingError>`.
Decoded images are immutable. A recorded picture retains its images and text layouts.

```dream
import system;
import system.drawing;

async fun draw_label(): Result<byte[], DrawingError> {
    let canvas = Canvas(320, 100);
    let image = Image.from_rgba(1, 1, [(byte)40, 100, 200, 255]).await?;
    canvas.draw_image(image, Rect(0, 0, 320, 100));

    let style = TextStyle(24, Color.white);
    style.max_width = 280;
    canvas.fill_text("Hello, world!", 20, 20, style);

    let surface = DrawingSurface.create(320, 100).await?;
    surface.render(canvas)?;
    return surface.read_pixels().await;
}
```

Text positions identify the top-left of the paragraph. Text shaping handles ligatures, text direction,
wrapping and alignment. The bundled Noto Sans supplies Latin, Greek, Cyrillic and Devanagari glyphs;
other scripts require a font containing their glyphs. Native rendering can use installed fonts.
`Fonts.register(font_bytes).await` registers a font and returns its first family name, which can be
assigned to `style.family`. Registered fonts are available to subsequent layouts on that drawing host.

For a frequently reused label, create `TextLayout.create(text, style).await`, inspect its `width` and
`height`, and draw it with `canvas.fill_text(layout, x, y, color)`. Layout captures the style's font
and paragraph settings; the draw call supplies its color. The simple string overload caches shaping
internally. `draw_image` accepts an optional source rectangle, opacity and nearest-neighbor filtering.

## Existing GPU windows

Import `system.drawing.gpu` to display a drawing through an existing `GpuSurface` with
`drawing.present(surface).await`. The drawing and window surface dimensions must match. The first
presentation creates a pipeline and texture; later calls reuse them. `close()` releases
those cached resources. Presenting to another surface replaces the cache.

```dream
import system;
import system.drawing;
import system.drawing.gpu;
import system.gpu;
import system.window;

async fun main(): void {
    let window = Window.open(WindowDesc("Canvas UI", 640, 400).resizable(false)).unwrap();
    let device = GpuDevice.create(GpuBackend.Auto).await.unwrap();
    let screen = device.create_surface(window).unwrap();
    let drawing = DrawingSurface.create(screen.width, screen.height, DrawingBackend.Cpu)
        .await
        .unwrap();
    let canvas = Canvas(screen.width, screen.height);
    canvas.clear(Color.rgb(30, 40, 60));
    canvas.fill_text("Settings", 20, 20, TextStyle(24, Color.white));
    drawing.render(canvas).unwrap();

    while window.is_open {
        window.poll();
        if !window.is_open {
            break;
        }
        drawing.present(screen).await.unwrap();
    }
    drawing.close();
}
```

This opens a native window, or creates a canvas when compiled for a browser. To use an existing
HTML canvas, use `Window.canvas("canvas-id")` instead of `Window.open`. The example stops on setup
or rendering errors through `unwrap`; handle each returned `Result` to recover in an application.
For resizable windows, pass `WindowEvent.Resized` sizes to `screen.resize`, then recreate the
`DrawingSurface` and `Canvas` at the new physical dimensions before presenting again.

Presentation replaces the entire window image. Use `copy_to(texture).await` instead when your own
GPU pipeline composites a UI texture with other rendering. In a browser, GPU drawings share the
window device and transfer on the GPU, without a pixel download or CPU upload. The first transfer
to a device replays the recorded picture there; later renders and transfers reuse that device.
Transferring to another device replays the picture again, preserving its recorded configuration.
RGBA textures copy directly; BGRA textures use a cached GPU conversion pass. Copies preserve the
stored sRGB bytes and premultiplied alpha, including sRGB texture formats. The destination must
match the drawing dimensions and be single-sampled.

On Windows, GPU drawings prefer D3D12. When the destination uses D3D12 or Vulkan on the same adapter with shared-resource support,
`present` and `copy_to` transfer through a reusable shared texture with GPU fence handoffs. They
do not download or re-upload canvas pixels. RGBA and BGRA copies preserve the stored sRGB bytes
and premultiplied alpha. Changing the transfer size or RGBA/BGRA family while a previous copy is
still executing returns a `DrawingError`; wait for the previous GPU work before retrying.
The first Vulkan transfer replays the recorded picture on a matching Vulkan device. Changing
between D3D12 and Vulkan replays it again, preserving the recorded configuration and assets.

CPU drawings upload pixels. Other native combinations currently download then upload GPU drawing
pixels; use CPU drawing there to avoid the download. Explicit `read_pixels` always remains a
synchronization point. Configure your consuming pipeline for premultiplied alpha when compositing
UI textures.

## A complete visual card

First record the card. The isolated outer group supplies the shadow. Its backdrop samples the
colored shapes drawn inside that same group; the gradient outline and filtered label stay sharp.
The logical 640 by 400 composition is scaled to the physical surface size.

```dream
import system;
import system.drawing;
import system.drawing.gpu;
import system.gpu;
import system.window;

fun card(width: int, height: int): Canvas {
    let canvas = Canvas(width, height);
    canvas.clear(Color.rgb(24, 30, 48));
    canvas.scale((float)width / 640.0f, (float)height / 400.0f);
    canvas.fill_circle(550, 340, 180, Color.rgb(42, 64, 110));
    canvas.save_layer(effects: Option.Some(Effects().shadow(0, 12, 18, Color.rgba(0, 0, 0, 0.4f))));
    canvas.fill_rect(40, 40, 560, 320, Color.rgb(38, 58, 88), 24);
    canvas.save();
    canvas.clip(DrawingPath.rect(Rect(40, 40, 560, 320), 24));
    canvas.fill_circle(490, 95, 110, Color.rgb(90, 100, 220));
    canvas.fill_circle(170, 310, 140, Color.rgb(30, 140, 180));
    let panel = Rect(64, 64, 512, 272);
    canvas.backdrop(
        panel,
        Backdrop.blur(18).tint(Color.rgba(255, 255, 255, 0.12f)).noise(0.04f).rounded(16)
    );
    let outline = Paint.linear(
        64,
        64,
        576,
        336,
        [
            GradientStop(0, Color.rgba(255, 255, 255, 0.7f)),
            GradientStop(0.5f, Color.rgba(150, 200, 255, 0.3f)),
            GradientStop(1, Color.rgba(255, 255, 255, 0.1f))
        ]
    );
    let stroke = StrokeStyle(2);
    stroke.join = StrokeJoin.Round;
    canvas.stroke(DrawingPath.rect(panel, 16), outline, stroke);
    canvas.save_layer(effects: Option.Some(Effects().saturation(0.85f).brightness(1.05f)));
    canvas.fill_text("Canvas UI", 96, 92, TextStyle(32, Color.white));
    canvas.fill_text(
        "Blur, grain, shadows and gradient outlines",
        96,
        148,
        TextStyle(18, Color.rgb(225, 235, 255))
    );
    canvas.fill_rect(96, 230, 180, 48, Color.rgb(80, 120, 220), 12);
    canvas.fill_text("Get started", 116, 240, TextStyle(20, Color.white));
    canvas.restore();
    canvas.restore();
    canvas.restore();
    return canvas;
}
```

Then create the existing window/GPU surface and present the card. This example uses CPU drawing
for portable window updates. In a browser or on Windows with a D3D12 or Vulkan window surface, change the
backend to `DrawingBackend.Gpu` to keep rendering and presentation on the GPU. `unwrap` stops on errors; inspect the
returned `Result` values in an application that needs recovery.

```dream
async fun main(): void {
    let window = Window.open(WindowDesc("Canvas UI visuals", 640, 400).resizable(false)).unwrap();
    let device = GpuDevice.create(GpuBackend.Auto).await.unwrap();
    let screen = device.create_surface(window).unwrap();
    let drawing = DrawingSurface.create(screen.width, screen.height, DrawingBackend.Cpu)
        .await
        .unwrap();
    drawing.render(card(screen.width, screen.height)).unwrap();
    while window.is_open {
        window.poll();
        if !window.is_open {
            break;
        }
        drawing.present(screen).await.unwrap();
    }
    drawing.close();
}
```

## Browser host

Browser rendering uses the same Vello engine in a separate drawing WASM host. Build it once:

```bash
cargo install wasm-bindgen-cli --version 0.2.129 --locked
python scripts/build-drawing-web.py
```

Compile the Dream program with `--runtime --web`. The compiler copies `dream_drawing.js` and
`dream_drawing_bg.wasm` next to the generated runtime. Deploy both companions and the accompanying `NotoSans-OFL.txt` license with it. Set
`DREAM_DRAWING_WEB` to a prebuilt host directory when building outside this checkout. The same host
supports CPU rendering under Node when emitted with `--runtime --node`.

## Current coverage

Shapes, Bézier paths, strokes, solid colors, linear and radial gradients, affine transforms,
nonzero/even-odd fills, clips, isolated opacity groups, backdrop blur, PNG/JPEG images, shaped text,
group filters, shadows, acrylic noise, nested backdrops, pixel reads and existing GPU window presentation are supported. SVG, additional blend modes,
arbitrary filter graphs and direct GPU texture sharing remain future work.
