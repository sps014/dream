# Paint

**Import:** `import system.drawing;`

## `Paint`

A reusable solid color or multi-stop gradient, captured when a draw is recorded.

```dream
public class Paint
```

## `solid`

Creates a solid paint.

```dream
public static fun solid(color: Color): Paint
```

## `linear`

Creates a linear gradient between the endpoints. Requires 2..256 sorted stops in 0..1; colors outside the stop range extend to the edges.

```dream
public static fun linear(x0: float, y0: float, x1: float, y1: float, stops: GradientStop[]): Paint
```

## `radial`

Creates a circular gradient from the center to a positive radius. Requires 2..256 sorted stops in 0..1.

```dream
public static fun radial(x: float, y: float, radius: float, stops: GradientStop[]): Paint
```

## `sweep`

Creates a clockwise angular gradient about the center, from start_angle to a greater end_angle in radians. Requires 2..256 sorted stops in 0..1.

```dream
public static fun sweep(
    x: float,
    y: float,
    start_angle: float,
    end_angle: float,
    stops: GradientStop[]
): Paint
```
