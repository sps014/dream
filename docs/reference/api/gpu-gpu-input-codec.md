# Gpu Input Codec

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `read_i32`

Reads a little-endian i32 at byte `off`.

```dream
public static fun read_i32(buf: byte[], off: int): int
```

## `read_f32`

Reads a little-endian f32 at byte `off`.

```dream
public static fun read_f32(buf: byte[], off: int): float
```

## `read_string`

Reads a length-prefixed UTF-8 string at byte `off`.

```dream
public static fun read_string(buf: byte[], off: int): string
```

## `string_byte_len`

Byte length of a length-prefixed string starting at `off` (header + payload).

```dream
public static fun string_byte_len(buf: byte[], off: int): int
```

## `decode_pointer`

Decodes a packed pointer latch blob into `GpuPointer`.

```dream
public static fun decode_pointer(buf: byte[]): GpuPointer
```

## `decode_pointers`

Decodes a packed multi-pointer latch (`u32 count` + `count` × 32-byte `GpuPointer` records).

```dream
public static fun decode_pointers(buf: byte[]): GpuPointer[]
```

## `decode_mods`

Decodes a packed modifier latch blob into `GpuMods`.

```dream
public static fun decode_mods(buf: byte[]): GpuMods
```

## `decode_events`

Decodes a packed event queue into `GpuInputEvent` values.

```dream
public static fun decode_events(buf: byte[]): GpuInputEvent[]
```
