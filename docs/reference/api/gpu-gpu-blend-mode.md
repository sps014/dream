# GpuBlendMode

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuBlendMode`

How a fragment shader's color combines with the color already in the target.

```dream
public enum GpuBlendMode
```

## `Replace`

The new color replaces the old one.

```dream
Replace = 0
```

## `Alpha`

Standard transparency: the new color is mixed in by its alpha.

```dream
Alpha = 1
```

## `Premultiplied`

Transparency for colors already multiplied by their alpha.

```dream
Premultiplied = 2
```

## `Additive`

The new color is added to the old one, for glows and particles.

```dream
Additive = 3
```
