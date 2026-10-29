# Color

**Import:** `import system.drawing;`

## `Color`

An sRGB color with straight alpha. RGB factories take byte-scale channels; alpha is 0..1.

```dream
public struct Color
```

## `rgb`

Creates an opaque color from red, green and blue values in 0..255.

```dream
public static fun rgb(red: int, green: int, blue: int): Color
```

## `rgba`

Creates a color from RGB values in 0..255 and opacity in 0..1. Invalid channels fail at render.

```dream
public static fun rgba(red: int, green: int, blue: int, alpha: float): Color
```

## `white`

Opaque white.

```dream
public static get white(): Color
```

## `black`

Opaque black.

```dream
public static get black(): Color
```

## `transparent`

Transparent black.

```dream
public static get transparent(): Color
```
