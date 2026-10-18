# GpuIndexFormat

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuIndexFormat`

The integer size of an index buffer's elements.

```dream
public enum GpuIndexFormat
```

## `Uint16`

16-bit indexes, from a `GpuBuffer<ushort>`.

```dream
Uint16 = 0
```

## `Uint32`

32-bit indexes, from a `GpuBuffer<uint>`.

```dream
Uint32 = 1
```
