---
title: Drawing
summary: Draw shapes, shadows, images, text and backdrop panels on CPU or GPU, and measure and hit-test text.
---

# Drawing

Import `system.drawing` to draw into an offscreen image. Create a canvas, record shapes, then render it into a CPU or GPU surface.

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

`DrawingBackend.Auto`, the default, tries the GPU and falls back to the CPU. Pass `DrawingBackend.Cpu` to skip the GPU, or `DrawingBackend.Gpu` to require one.

- `backend` reports which one was selected.
- Each side is 1 to 8192 pixels, and the total is at most 16,777,216. The canvas size must match the surface.

Coordinates start at the top-left, with positive y downward. RGB channels are 0 to 255, and alpha is a float from 0 to 1.

- `read_pixels` returns tightly packed premultiplied sRGB RGBA8, top row first. A GPU read waits for submitted work.
- Each render replaces the previous image, including transparent pixels.

## Backdrop panels

`canvas.backdrop(rect, effect)` filters earlier drawing in the current group, or the canvas root when no group is open. Parent groups and later siblings are left out.

- Blur may sample outside the panel. The result and tint are clipped to the panel and the current clip, and later drawing stays sharp.
- A sigma of 0 applies only the tint. Sigma is in physical pixels, from 0 to 128, and ignores the current transform.

The effect samples only this canvas, not the desktop or other windows.

- A picture allows at most 64 backdrop panels, including panels inside `draw_picture`.
- Each panel after the first in the same group renders the whole canvas again.
- An enclosing group applies its filters, opacity, and blending after nested panels are composed.

`.noise(opacity, seed: 0)` adds grain after blur and tint. Opacity is 0 to 1, and the panel's alpha is kept.

- The seed picks a fixed 64 by 64 tile, the same on CPU and GPU, anchored in device pixels so transforms do not move the grain.
- Later edits to a `Backdrop` do not change panels already recorded.

```dream
canvas.backdrop(
    Rect(40, 40, 320, 200),
    Backdrop.blur(20).tint(Color.rgba(255, 255, 255, 0.15f)).noise(0.04f).rounded(16)
);
```

## Paths, gradients and clipping

Use `DrawingPath` for a shape you draw more than once. A draw captures the path and paint at that moment.

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

Pair every `save` or `save_layer` with `restore`. `save_layer(opacity)` draws its contents as one group, so shapes inside share that opacity.

- Save depth and clip depth are limited to 64.
- `finish()` returns an immutable `Picture`, and rendering a canvas captures one automatically. `clear` drops the recording and resets canvas state.

Pass `CornerRadii(top_left, top_right, bottom_right, bottom_left)` to round each corner on its own. Radii larger than half the smaller side are clamped, and negative radii draw square corners.

```dream
let sheet = DrawingPath.rect(Rect(0, 120, 320, 120), CornerRadii(16, 16, 0, 0));
canvas.fill(sheet, Paint.solid(Color.white));
```

`finish` rejects unbalanced `save` and `restore`. Rendering also checks colors, paths, transforms, blur, and command limits, and returns `DrawingError` when they fail.

Geometry must be finite and within +/-1000000. A picture allows at most 4,000,000 encoded float values.

## Stroke styles and blending

Pass `StrokeStyle(width)` and a `Paint` to `canvas.stroke`. Cap, join, miter limit, dashes, and dash offset are captured when you record the stroke.

- Dash lengths must be positive. An empty list draws a solid stroke.
- A gradient accepts 2 to 256 stops, sorted from 0 to 1. Equal offsets are a hard edge, and `Paint.sweep` uses clockwise radians.

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

Blending applies to the whole group after its opacity, against earlier parent content. Modes are SourceOver, Multiply, Screen, Overlay, Darken, Lighten, Difference, and Exclusion; an invalid style returns `DrawingError` at render.

## Group filters and shadows

An `Effects()` chain runs in call order on an isolated group, before group opacity and parent blending. Pair `save_layer` with `restore`; the clip bounds the result.

```dream
let effects = Effects().shadow(0, 8, 16, Color.rgba(0, 0, 0, 0.25f)).saturation(0.8f);
canvas.save_layer(effects: Option.Some(effects));
canvas.fill_rect(40, 40, 240, 120, Color.white, 16);
canvas.restore();
```

`blur` and `shadow` use physical pixels and ignore transforms. Sigma is 0 to 128, and shadow offsets stay within +/-8192.

- A shadow includes the original picture.
- `brightness`, `contrast`, and `saturation` take 0 to 8. A factor of 1 leaves the input unchanged.
- Brightness multiplies straight sRGB channels, contrast scales distance from 0.5, and saturation scales distance from luminance.
- Channels are clamped after each step, then premultiplied again. Color steps keep alpha and leave transparent pixels colorless.

The recorded group keeps the chain from that moment. A later edit does not change it.

- A chain allows at most 16 operations. Invalid values, or more than 256 MiB of full-surface scratch, return `DrawingError` at render.
- `read_pixels` waits and downloads pixels. Native GPU presentation also reads pixels back.
- Inset shadows, spread, and arbitrary filter graphs are not available.

## Box shadows

`canvas.box_shadow(rect, radius, dx, dy, sigma, color, spread)` paints the soft shadow of a rounded
box in one step. The shadow is computed exactly from the rectangle, so it costs about as much as a
plain fill and needs no isolated group, which makes it the cheap way to give many cards elevation.

```dream
let card = Rect(40, 40, 240, 120);
canvas.box_shadow(card, 16, 0, 8, 12, Color.rgba(0, 0, 0, 0.25f));
canvas.fill_rect(card.x, card.y, card.width, card.height, Color.white, 16);
```

`rect` and `radius` describe the box, and `dx` and `dy` move the shadow. `spread` (default 0) grows or shrinks every side and changes the corner radius by the same amount.

- These values are canvas units and follow the current transform. Group shadows do not.
- `sigma` is the blur's standard deviation, from 0 to 128. Zero is a sharp shape.
- The shadow is also painted under the box, so draw the box afterward. A translucent box shows that shadow; inset shadows are not available.

## Reusing pictures

`canvas.draw_picture(picture)` draws a finished `Picture` under the current transform and clip. Record a part that rarely changes, then draw it every frame.

The nested `clear` color fills only that picture's bounds, and its drawing is clipped to them. A backdrop inside it filters what the outer canvas drew before it.

```dream
let badge = Canvas(48, 48);
badge.fill_circle(24, 24, 24, Color.rgb(220, 60, 60));
badge.fill_text("3", 18, 12, TextStyle(20, Color.white));
let icon = badge.finish()?;

for (let i = 0; i < 4; i = i + 1) {
    canvas.save();
    canvas.translate((float)(i * 64), 0);
    canvas.draw_picture(icon);
    canvas.restore();
}
```

Pictures nest at most eight levels deep. The outer picture keeps the nested picture's images and
text layouts alive, and each `draw_picture` copies the commands, so very large pictures drawn many
times count toward the 4000000-value limit.

## Images and text

`Image.decode(bytes).await` decodes PNG or JPEG, and `Image.from_rgba(width, height, bytes).await` builds an image from straight RGBA8. Both return `Result<Image, DrawingError>`.

The image cannot be changed later. A recorded picture keeps its images and text layouts.

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

A text position is the top-left of the paragraph. Shaping handles ligatures, direction, wrapping, and alignment.

- Bundled Noto Sans covers Latin, Greek, Cyrillic, and Devanagari. Other scripts need a font with those glyphs, and native rendering can use installed fonts.
- `Fonts.register(font_bytes).await` returns the first family name for `style.family`. Later layouts on that drawing host can use it.

For a label you draw often, call `TextLayout.create(text, style).await`, read `width` and `height`, and draw with `canvas.fill_text(layout, x, y, color)`.

- The layout keeps the font and paragraph settings. The draw call supplies the color.
- The string overload keeps the 256 most recent layouts, up to 16 MiB. Each distinct string is stored once per recording.
- `draw_image` takes an optional source rectangle, opacity, and nearest-neighbor filtering.

### Measuring text

`TextLayout.measure(text, style)` returns `TextMetrics` without keeping a layout: widest-line `width`, total `height`, `line_count`, and `baseline` (top to the first baseline). A `TextLayout` has the same `line_count` and `baseline`.

```dream
let style = TextStyle(14, Color.black);
style.max_width = 200;
let size = TextLayout.measure("A paragraph that wraps onto several lines", style)?;
canvas.fill_rect(10, 10, size.width + 16, size.height + 16, Color.rgb(240, 240, 240), 8);
canvas.fill_text("A paragraph that wraps onto several lines", 18, 18, style);
```

Measuring and then drawing the same text with the same style shares one cached layout. An invalid style returns `EINVAL`.

In browsers and Node, the drawing engine loads in the background at startup. Measure only after awaiting `TextLayout.create` or `DrawingSurface.create`; an earlier call returns `EUNAVAILABLE`.

### Carets and selections

A `TextLayout` answers the questions a text field asks. Positions are UTF-16 code unit offsets, the
same units as `string.length`: 0 is before the first character and `text.length` after the last.

- `index_at(x, y)` returns the position nearest to a point relative to the layout's top-left
  corner, so a click anywhere, even beside or below the text, places a caret.
- `caret(index)` returns a `Rect` one unit wide and as tall as the line, before that position.
- `selection(start, end)` returns a `List<Rect>` with one highlight per line fragment. The two
  positions may come in either order; an empty range returns no rectangles.

```dream
let field = TextLayout.create("Hello, world", TextStyle(16, Color.black)).await?;
let clicked = field.index_at(pointer_x - 20, pointer_y - 20);
for (let highlight in field.selection(selection_start, clicked)) {
    canvas.fill_rect(
        highlight.x + 20,
        highlight.y + 20,
        highlight.width,
        highlight.height,
        Color.rgba(60, 120, 255, 0.3f)
    );
}
canvas.fill_text(field, 20, 20, Color.black);
let caret = field.caret(clicked);
canvas.fill_rect(caret.x + 20, caret.y + 20, caret.width, caret.height, Color.black);
```

A position past the end clamps to the end, and a position inside a surrogate pair moves to the start
of that character.

## Existing GPU windows

Import `system.drawing.gpu` and call `drawing.present(surface).await` to show a drawing on an existing `GpuSurface`. The drawing and the surface must be the same size.

- The first present creates a pipeline and texture, and later calls reuse them. `close()` releases that cache.
- Presenting to another surface replaces the cache.

```dream
import system;
import system.drawing;
import system.drawing.gpu;
import system.gpu;
import system.window;

async fun main(): void {
    let window = Window.open(WindowOptions("Canvas UI", 640, 400).resizable(false)).unwrap();
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

This opens a native window, or a canvas in the browser. Use `Window.canvas("canvas-id")` for an existing HTML canvas.

The example uses `unwrap` and stops on errors. For a resizable window, pass `WindowEvent.Resized` sizes to `screen.resize`, then recreate the `DrawingSurface` and `Canvas`.

`present` replaces the whole window image. Use `copy_to(texture).await` when your own pipeline composites the UI with other rendering.

- In a browser, a GPU drawing stays on the GPU. The first transfer to a device replays the picture, later transfers reuse it, and another device replays it again.
- RGBA copies directly, and BGRA uses a cached conversion. Bytes stay sRGB and premultiplied; the destination must match the drawing size and be single-sampled.

CPU drawings upload pixels. Native GPU drawings currently download pixels and upload them again, so use CPU drawing for native window updates.

`read_pixels` always waits for the GPU. Set your pipeline to premultiplied alpha when you composite a UI texture.

## A complete visual card

The outer group supplies the shadow, and its backdrop samples shapes drawn in that group. The outline and label stay sharp, and the 640 by 400 layout is scaled to the surface.

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

This example uses CPU drawing so native presentation does not read pixels back. In a browser, use `DrawingBackend.Gpu` to stay on the GPU; `unwrap` stops on errors.

```dream
async fun main(): void {
    let window = Window.open(WindowOptions("Canvas UI visuals", 640, 400).resizable(false)).unwrap();
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

Browser rendering uses a separate drawing WASM host. Build it once:

```bash
cargo install wasm-bindgen-cli --version 0.2.129 --locked
python scripts/build-drawing-web.py
```

Compile with `--runtime --web`. The compiler copies `dream_drawing.js` and `dream_drawing_bg.wasm` next to the runtime.

Deploy both files and `NotoSans-OFL.txt` with the page. Set `DREAM_DRAWING_WEB` outside this checkout, and use `--runtime --node` for CPU rendering on Node.

## Current coverage

Shapes, Bézier paths, per-corner rounded rectangles, strokes, solid colors, linear and radial
gradients, affine transforms, nonzero/even-odd fills, clips, isolated opacity groups, backdrop blur,
PNG/JPEG images, shaped text with measurement and hit-testing, group filters, shadows, analytic box
shadows, nested pictures, acrylic noise, nested backdrops, pixel reads and existing GPU window
presentation are supported. SVG, additional blend modes,
arbitrary filter graphs and direct GPU texture sharing remain future work.
