# Drawing Backend

**Import:** `import system.drawing;`

## `DrawingBackend`

Where a drawing surface rasterizes and composites its canvas.

```dream
public enum DrawingBackend
```

## `Auto`

Attempts GPU initialization, then uses CPU if no suitable GPU is available.

```dream
Auto = 0
```

## `Cpu`

Uses Vello software rendering without requiring a GPU.

```dream
Cpu = 1
```

## `Gpu`

Requires Vello GPU rendering; initialization fails if unavailable.

```dream
Gpu = 2
```
