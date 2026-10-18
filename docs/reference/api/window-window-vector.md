# WindowVector

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `WindowVector`

A 2D position or offset in window coordinates.  Window coordinates are logical pixels measured from the top-left corner of the window's content area, with `y` growing downward. Multiply by `Window.scale_factor` to get physical pixels.

```dream
public struct WindowVector
```

## `x`

Horizontal component.

```dream
public x: float
```

## `y`

Vertical component; positive values point down.

```dream
public y: float
```

## `constructor`

Creates a vector from its two components.

```dream
public constructor(x: float, y: float)
```
