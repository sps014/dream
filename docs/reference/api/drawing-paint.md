# Paint

**Import:** `import system.drawing;`

## `Paint`

A reusable solid color or two-color gradient, captured when a draw is recorded.

```dream
public class Paint
```

## `solid`

Creates a solid paint.

```dream
public static fun solid(color: Color): Paint
```

## `linear`

Creates a gradient from start color at (x0, y0) to end color at (x1, y1).

```dream
public static fun linear(
    x0: float,
    y0: float,
    x1: float,
    y1: float,
    start: Color,
    end: Color
): Paint
```

## `radial`

Creates a circular gradient from the center color to the edge at a positive radius.

```dream
public static fun radial(x: float, y: float, radius: float, start: Color, end: Color): Paint
```
