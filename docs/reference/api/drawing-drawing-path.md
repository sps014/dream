# Drawing Path

**Import:** `import system.drawing;`

## `DrawingPath`

Reusable vector geometry. Mutating a path does not change commands already recorded with it.

```dream
public class DrawingPath
```

## `constructor`

Creates an empty path. Begin each contour with move_to.

```dream
public constructor()
```

## `move_to`

Starts a contour at the given coordinates and returns this path.

```dream
public fun move_to(x: float, y: float): DrawingPath
```

## `line_to`

Adds a straight segment to the given coordinates and returns this path.

```dream
public fun line_to(x: float, y: float): DrawingPath
```

## `quad_to`

Adds a quadratic curve with one control point and returns this path.

```dream
public fun quad_to(cx: float, cy: float, x: float, y: float): DrawingPath
```

## `curve_to`

Adds a cubic curve with two control points and returns this path.

```dream
public fun curve_to(ax: float, ay: float, bx: float, by: float, x: float, y: float): DrawingPath
```

## `close`

Closes the current contour and returns this path.

```dream
public fun close(): DrawingPath
```

## `rect`

Creates a closed rectangle. Radius rounds its corners and is clamped to half its smaller side.

```dream
public static fun rect(rect: Rect, radius: float = 0.0f): DrawingPath
```

## `ellipse`

Creates a closed ellipse inscribed in the rectangle.

```dream
public static fun ellipse(rect: Rect): DrawingPath
```
