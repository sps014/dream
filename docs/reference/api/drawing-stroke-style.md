# Stroke Style

**Import:** `import system.drawing;`

## `StrokeStyle`

Stroke geometry, captured by Canvas.stroke when recorded. Invalid values fail at render.

```dream
public class StrokeStyle
```

## `width`

Positive stroke width in canvas units.

```dream
public width: float
```

## `cap`

End treatment; defaults to Butt.

```dream
public cap: StrokeCap
```

## `join`

Corner treatment; defaults to Miter.

```dream
public join: StrokeJoin
```

## `miter_limit`

Maximum miter length divided by half the stroke width; at least one, default four.

```dream
public miter_limit: float
```

## `dashes`

Alternating positive dash and gap lengths in canvas units, at most 256. Empty draws a solid stroke; an odd list repeats twice.

```dream
public dashes: float[]
```

## `dash_offset`

Distance into the repeating dash pattern in canvas units; defaults to zero.

```dream
public dash_offset: float
```

## `constructor`

Creates a stroke with butt caps, miter joins, miter limit four and no dashes.

```dream
public constructor(width: float)
```
