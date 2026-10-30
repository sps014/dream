# Backdrop

**Import:** `import system.drawing;`

## `Backdrop`

Filters content drawn earlier in the canvas. Later drawings and other windows are excluded.

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

## `rounded`

Sets the panel's corner radius in canvas units and returns this configuration.

```dream
public fun rounded(radius: float): Backdrop
```
