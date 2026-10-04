# Gpu Cmd

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `current`

```dream
public static fun current(): int
```

## `begin`

```dream
public static fun begin(): GpuCmdStream
```

## `i32`

```dream
public fun i32(v: int): void
```

## `f32`

```dream
public fun f32(v: float): void
```

## `op`

```dream
public fun op(o: GpuCmdOp): void
```

## `load_op`

```dream
public fun load_op(o: GpuLoadOp): void
```

## `store_op`

```dream
public fun store_op(o: GpuStoreOp): void
```

## `i32_array`

```dream
public fun i32_array(a: int[]): void
```

## `blob`

Length-prefixed payload, zero-padded so the following record stays 4-byte aligned.

```dream
public fun blob(data: byte[]): void
```

## `bytes`

```dream
public fun bytes(): byte[]
```
