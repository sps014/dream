# GpuCullMode

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuCullMode`

Which triangles a render pipeline skips, judged by their winding order on screen. Counter-clockwise triangles face the viewer.

```dream
public enum GpuCullMode
```

## `None`

Draws every triangle.

```dream
None = 0
```

## `Front`

Skips triangles that face the viewer.

```dream
Front = 1
```

## `Back`

Skips triangles that face away from the viewer.

```dream
Back = 2
```
