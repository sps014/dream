# GpuVertexFormat

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuVertexFormat`

The type of one vertex attribute in a vertex buffer.

```dream
public enum GpuVertexFormat
```

## `Float32`

One `float`.

```dream
Float32 = 0
```

## `Float32x2`

Two `float`s, read as `GpuVec2`.

```dream
Float32x2 = 1
```

## `Float32x3`

Three `float`s, read as `GpuVec3`.

```dream
Float32x3 = 2
```

## `Float32x4`

Four `float`s, read as `GpuVec4`.

```dream
Float32x4 = 3
```

## `Sint32`

One `int`.

```dream
Sint32 = 4
```

## `Uint32`

One `uint`.

```dream
Uint32 = 5
```

## `Unorm8x4`

Four bytes read as a `GpuVec4` from `0.0` to `1.0`, common for packed colors.

```dream
Unorm8x4 = 6
```
