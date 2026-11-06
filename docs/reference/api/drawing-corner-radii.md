# Corner Radii

**Import:** `import system.drawing;`

## `CornerRadii`

Corner radii for a rounded rectangle, in canvas units. Each radius is clamped to half the rectangle's smaller side when drawn.

```dream
public struct CornerRadii
```

## `top_left`

Radius of the top-left corner.

```dream
public top_left: float
```

## `top_right`

Radius of the top-right corner.

```dream
public top_right: float
```

## `bottom_right`

Radius of the bottom-right corner.

```dream
public bottom_right: float
```

## `bottom_left`

Radius of the bottom-left corner.

```dream
public bottom_left: float
```

## `constructor`

Creates radii in clockwise order starting at the top-left corner. Zero gives a square corner.

```dream
public constructor(top_left: float, top_right: float, bottom_right: float, bottom_left: float)
```
