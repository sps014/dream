# Effects

**Import:** `import system.drawing;`

## `Effects`

An ordered chain of group filters. Canvas.save_layer captures it immediately. At most 16 operations; render returns DrawingError for invalid parameters.

```dream
public class Effects
```

## `constructor`

Creates an empty chain that leaves group pixels unchanged.

```dream
public constructor()
```

## `blur`

Appends Gaussian blur with sigma in 0..128 physical pixels, independent of the canvas transform.

```dream
public fun blur(sigma: float): Effects
```

## `shadow`

Appends a colored shadow behind the original input. Offsets are physical pixels within +/-8192; sigma is 0..128 physical pixels. Includes the original input.

```dream
public fun shadow(dx: float, dy: float, sigma: float, color: Color): Effects
```

## `brightness`

Multiplies straight sRGB channels by factor in 0..8; one preserves brightness. Alpha is unchanged.

```dream
public fun brightness(factor: float): Effects
```

## `contrast`

Scales straight sRGB channel distance from 0.5 by factor in 0..8; one preserves contrast. Alpha is unchanged.

```dream
public fun contrast(factor: float): Effects
```

## `saturation`

Scales straight sRGB distance from luminance (0.2126 R + 0.7152 G + 0.0722 B) by factor in 0..8. Zero gives grayscale, one preserves saturation; alpha is unchanged.

```dream
public fun saturation(factor: float): Effects
```
