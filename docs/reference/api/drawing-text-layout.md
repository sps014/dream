# Text Layout

**Import:** `import system.drawing;`

## `TextLayout`

Immutable shaped text for repeated labels and editable text. Layout metrics and glyph positions are shared between CPU and GPU rendering.  Text positions are UTF-16 code unit offsets, the same units as `string.length` and `char_at`. Offset 0 is before the first character and `text.length` is after the last.

```dream
public class TextLayout: DrawingResource
```

## `create`

Shapes and lays out text with the supplied style, capturing its font and paragraph settings. Color is supplied separately when drawing.

```dream
public static async fun create(text: string, style: TextStyle): Result<TextLayout, DrawingError>
```

## `measure`

Measures text with the supplied style without keeping a layout. Results are cached with the same layouts that canvas text uses, so measuring and then drawing the same text shapes it once.  Fails with `EINVAL` for invalid style values or text longer than 262144 bytes. In browsers and Node, the drawing engine loads in the background when the program starts; a call made before it is ready fails with `EUNAVAILABLE`, and awaiting any async drawing call, such as `TextLayout.create` or `DrawingSurface.create`, guarantees it is ready.

```dream
public static fun measure(text: string, style: TextStyle): Result<TextMetrics, DrawingError>
```

## `width`

Laid-out text width in canvas units.

```dream
public get width(): float
```

## `height`

Laid-out text height in canvas units.

```dream
public get height(): float
```

## `line_count`

Number of lines after wrapping and explicit newlines.

```dream
public get line_count(): int
```

## `baseline`

Distance from the top of the layout to the first line's baseline, in canvas units.

```dream
public get baseline(): float
```

## `index_at`

Returns the text position nearest to a point in layout coordinates, relative to the layout's top-left origin. Points above, below or beside the text resolve to the nearest line and line edge, so a click anywhere can place a caret.

```dream
public fun index_at(x: float, y: float): int
```

## `caret`

Returns the caret rectangle before a text position: one canvas unit wide and as tall as its line. Positions past the end clamp to the end, and a position inside a surrogate pair moves to the start of that character.

```dream
public fun caret(index: int): Rect
```

## `selection`

Returns highlight rectangles covering the text between two positions, one per line fragment, in visual order. The positions may be given in either order; an empty range returns no rectangles.

```dream
public fun selection(start: int, end: int): List<Rect>
```
