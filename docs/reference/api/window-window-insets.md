# WindowInsets

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for touch input and safe-area behavior. See [Reading API signatures](index.md#reading-a-signature) for signature notation.

## `WindowInsets`

Distances from the drawable area's edges, in logical pixels, that content should avoid.

```dream
public struct WindowInsets
```

## `top`

Distance below the top edge, such as the area occupied by a display cutout.

```dream
public top: float
```

## `right`

Distance to the left of the right edge.

```dream
public right: float
```

## `bottom`

Distance above the bottom edge, such as a system gesture area.

```dream
public bottom: float
```

## `left`

Distance to the right of the left edge.

```dream
public left: float
```

## `constructor`

Creates edge distances in top, right, bottom, left order. Every distance defaults to zero.

```dream
public constructor(
    top: float = 0.0f,
    right: float = 0.0f,
    bottom: float = 0.0f,
    left: float = 0.0f
)
```
