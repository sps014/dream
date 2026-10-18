# GpuFilter

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuFilter`

How a sampler picks a color between texel centers.

```dream
public enum GpuFilter
```

## `Nearest`

Uses the closest texel, for a sharp pixelated look.

```dream
Nearest = 0
```

## `Linear`

Blends the surrounding texels, for smooth results.

```dream
Linear = 1
```
