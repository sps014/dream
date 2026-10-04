# Uniforms

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Uniforms`

Pack little-endian uniforms for `Compute.run_*` / draw helpers.

```dream
public static class Uniforms
```

## `pack`

```dream
public static fun pack<T : unmanaged>(value: T): byte[]
```

## `pack_i32`

Packs `int` values as little-endian i32 bytes.

```dream
public static fun pack_i32(values: int[]): byte[]
```

## `pack_f32`

Packs `float` values as IEEE-754 little-endian f32 bytes.

```dream
public static fun pack_f32(values: float[]): byte[]
```
