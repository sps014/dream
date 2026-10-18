# LocationAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `LocationAttribute`

Numbers a value passed between pipeline stages: a vertex shader output and the fragment shader input with the same `index` carry the same value, interpolated across the triangle. On a fragment shader's return struct, `index` selects the color target. Indexes start at `0` and must be unique within a struct.

```dream
public struct LocationAttribute
```

## `index`

The slot number, starting at `0`.

```dream
public index: int
```
