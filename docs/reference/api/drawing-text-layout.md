# Text Layout

**Import:** `import system.drawing;`

## `TextLayout`

Immutable shaped text for repeated labels. Layout metrics and glyph positions are shared between CPU and GPU rendering.

```dream
public class TextLayout: DrawingResource
```

## `create`

Shapes and lays out text with the supplied style, capturing its font and paragraph settings. Color is supplied separately when drawing.

```dream
public static async fun create(text: string, style: TextStyle): Result<TextLayout, DrawingError>
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
