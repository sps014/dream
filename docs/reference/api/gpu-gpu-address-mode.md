# GpuAddressMode

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuAddressMode`

What a sampler reads for coordinates outside `0.0` to `1.0`.

```dream
public enum GpuAddressMode
```

## `ClampToEdge`

Repeats the edge texels.

```dream
ClampToEdge = 0
```

## `Repeat`

Tiles the texture.

```dream
Repeat = 1
```

## `MirrorRepeat`

Tiles the texture, mirroring every other copy.

```dream
MirrorRepeat = 2
```
