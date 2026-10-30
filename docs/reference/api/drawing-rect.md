# Rect

**Import:** `import system.drawing;`

## `Rect`

An axis-aligned rectangle in canvas coordinates, measured from its top-left corner.

```dream
public struct Rect
```

## `x`

Left edge.

```dream
public x: float
```

## `y`

Top edge.

```dream
public y: float
```

## `width`

Width; positive values draw toward the right.

```dream
public width: float
```

## `height`

Height; positive values draw downward.

```dream
public height: float
```

## `constructor`

Creates a rectangle from position and dimensions in canvas units.

```dream
public constructor(x: float, y: float, width: float, height: float)
```
