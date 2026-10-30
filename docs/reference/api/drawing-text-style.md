# Text Style

**Import:** `import system.drawing;`

## `TextStyle`

Text appearance and paragraph layout. Canvas calls capture these values when recorded.

```dream
public class TextStyle
```

## `size`

Font size in canvas units, from 0.1 to 4096.

```dream
public size: float
```

## `color`

Solid text color.

```dream
public color: Color
```

## `family`

Font family name; defaults to the bundled Noto Sans. Native programs may also use installed fonts.

```dream
public family: string
```

## `weight`

Font weight from 1 to 1000; defaults to regular, 400.

```dream
public weight: float
```

## `max_width`

Wrapping width in canvas units. Zero disables automatic wrapping; explicit newlines still break lines.

```dream
public max_width: float
```

## `alignment`

Paragraph alignment within max_width, interpreted relative to text direction.

```dream
public alignment: TextAlignment
```

## `constructor`

Creates regular text with a size and color. Noto Sans is available on native and browser targets.

```dream
public constructor(size: float, color: Color)
```
