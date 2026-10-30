# Backdrop

**Import:** `import system.drawing;`

## `Backdrop`

Filters earlier content in the current isolated group, or the canvas root. Parent groups, later drawings and other windows are excluded.

```dream
public class Backdrop
```

## `blur`

Creates a Gaussian backdrop blur. Sigma is 0..128 physical pixels; zero only applies tint.

```dream
public static fun blur(sigma: float): Backdrop
```

## `tint`

Sets the color composited over the filtered backdrop and returns this configuration.

```dream
public fun tint(color: Color): Backdrop
```

## `noise`

Adds monochrome grain after blur and tint, preserving the panel alpha. Opacity is 0..1. A fixed seed (default zero) produces the same 64-pixel repeating tile, anchored in device pixels.

```dream
public fun noise(opacity: float, seed: int = 0): Backdrop
```

## `rounded`

Sets the panel's corner radius in canvas units and returns this configuration.

```dream
public fun rounded(radius: float): Backdrop
```
