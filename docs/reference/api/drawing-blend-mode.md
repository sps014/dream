# Blend Mode

**Import:** `import system.drawing;`

## `BlendMode`

How an isolated group combines with earlier parent content.

```dream
public enum BlendMode
```

## `SourceOver`

Composites the group over earlier content.

```dream
SourceOver
```

## `Multiply`

Multiplies source and destination colors.

```dream
Multiply
```

## `Screen`

Combines colors using their inverse product.

```dream
Screen
```

## `Overlay`

Multiplies dark destination colors and screens light ones.

```dream
Overlay
```

## `Darken`

Selects the darker source or destination channel.

```dream
Darken
```

## `Lighten`

Selects the lighter source or destination channel.

```dream
Lighten
```

## `Difference`

Uses the absolute difference of source and destination channels.

```dream
Difference
```

## `Exclusion`

Uses a lower-contrast difference blend.

```dream
Exclusion
```
