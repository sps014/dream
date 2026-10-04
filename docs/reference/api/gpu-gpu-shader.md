# GpuShader

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GpuShader`

Raw WGSL shader handle (escape hatch).

```dream
public class GpuShader
```

## `id`

```dream
public get id(): int
```

## `from_wgsl`

Compiles WGSL `source` and binds entry point `entry`.

```dream
public static fun from_wgsl(source: string, entry: string): GpuShader
```
