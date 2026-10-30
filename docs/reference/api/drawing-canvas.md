# Canvas

**Import:** `import system.drawing;`

## `Canvas`

Records 2D drawing in top-left coordinates with positive y downward. Calls execute at render.

```dream
public class Canvas
```

## `constructor`

Creates a recording canvas of the given physical pixel size, initially transparent.

```dream
public constructor(width: int, height: int)
```

## `clear`

Discards recorded drawing and state, then starts with the supplied background color.

```dream
public fun clear(color: Color): void
```

## `fill_rect`

Fills a rectangle with a solid color. Radius optionally rounds its corners.

```dream
public fun fill_rect(
    x: float,
    y: float,
    width: float,
    height: float,
    color: Color,
    radius: float = 0.0f
): void
```

## `fill_circle`

Fills a circle with the given center, radius and solid color.

```dream
public fun fill_circle(x: float, y: float, radius: float, color: Color): void
```

## `fill`

Fills vector geometry with a paint. even_odd selects alternating contour fill.

```dream
public fun fill(borrow path: DrawingPath, borrow paint: Paint, even_odd: bool = false): void
```

## `stroke`

Strokes a path with a solid or gradient paint and explicit stroke geometry. Captures paint, path and style immediately; render validates them.

```dream
public fun stroke(borrow path: DrawingPath, borrow paint: Paint, borrow style: StrokeStyle): void
```

## `save`

Saves the current transform and clip. Pair with restore before rendering.

```dream
public fun save(): void
```

## `save_layer`

Starts an isolated group. restore applies captured effects in order, then opacity in 0..1 and blending into the parent. The current clip also bounds the filtered output.

```dream
public fun save_layer(
    opacity: float = 1.0f,
    blend: BlendMode = BlendMode.SourceOver,
    effects: Option<Effects> = Option.None
): void
```

## `restore`

Restores the last saved transform and clip, and ends an isolated group if one was saved.

```dream
public fun restore(): void
```

## `transform`

Concatenates an affine matrix: x' = a*x + c*y + tx, y' = b*x + d*y + ty.

```dream
public fun transform(a: float, b: float, c: float, d: float, tx: float, ty: float): void
```

## `translate`

Moves subsequent drawing by the given canvas offsets.

```dream
public fun translate(x: float, y: float): void
```

## `scale`

Scales subsequent drawing along each axis. Negative factors mirror the drawing.

```dream
public fun scale(x: float, y: float): void
```

## `rotate`

Rotates subsequent drawing clockwise about the origin by an angle in radians.

```dream
public fun rotate(radians: float): void
```

## `clip`

Intersects the current clip with the nonzero-filled path in the current transform.

```dream
public fun clip(borrow path: DrawingPath): void
```

## `backdrop`

Filters earlier canvas content inside a panel and applies its tint. At most eight panels per picture; unavailable inside save_layer.

```dream
public fun backdrop(rect: Rect, borrow effect: Backdrop): void
```

## `finish`

Captures an immutable picture. Invalid dimensions or unbalanced save/restore return an error; render validates geometry and colors.

```dream
public fun finish(): Result<Picture, DrawingError>
```

## `draw_image`

Draws an image into a destination rectangle. Optional source crops in image pixels; opacity is 0..1 and nearest enables pixel-art sampling.

```dream
public fun draw_image(
    borrow image: Image,
    target: Rect,
    source: Option<Rect> = Option.None,
    opacity: float = 1.0f,
    nearest: bool = false
): void
```

## `fill_text`

Draws shaped text at a top-left origin. Text and style are captured immediately; matching layouts are cached during rendering.

```dream
public fun fill_text(text: string, x: float, y: float, borrow style: TextStyle): void
```

## `fill_text`

Draws an immutable precomputed layout at a top-left origin using the supplied color.

```dream
public fun fill_text(borrow layout: TextLayout, x: float, y: float, color: Color): void
```
