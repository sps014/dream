# Gradient Stop

**Import:** `import system.drawing;`

## `GradientStop`

A gradient color at an offset from zero to one. Equal offsets create a hard transition.

```dream
public struct GradientStop
```

## `offset`

Position along the gradient in 0..1. Stops must be sorted.

```dream
public offset: float
```

## `color`

Color at this position.

```dream
public color: Color
```

## `constructor`

Creates a stop. Rendering validates offsets and colors.

```dream
public constructor(offset: float, color: Color)
```
