# GpuTopology

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuTopology`

How a draw groups vertices into primitives.

```dream
public enum GpuTopology
```

## `TriangleList`

Every three vertices form a separate triangle.

```dream
TriangleList = 0
```

## `TriangleStrip`

Each vertex after the first two forms a triangle with the previous two.

```dream
TriangleStrip = 1
```

## `LineList`

Every two vertices form a separate line.

```dream
LineList = 2
```

## `LineStrip`

Each vertex after the first forms a line with the previous one.

```dream
LineStrip = 3
```

## `PointList`

Each vertex is a single point.

```dream
PointList = 4
```
