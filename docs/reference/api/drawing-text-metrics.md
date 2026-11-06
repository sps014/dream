# Text Metrics

**Import:** `import system.drawing;`

## `TextMetrics`

Size of laid-out text, measured without creating a reusable layout.

```dream
public struct TextMetrics
```

## `width`

Width of the widest line in canvas units.

```dream
public width: float
```

## `height`

Total height of all lines in canvas units.

```dream
public height: float
```

## `line_count`

Number of lines after wrapping and explicit newlines. Empty text has at most one line.

```dream
public line_count: int
```

## `baseline`

Distance from the top of the text to the first line's baseline, in canvas units; zero for empty text.

```dream
public baseline: float
```
