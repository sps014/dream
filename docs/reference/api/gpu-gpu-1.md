# Gpu

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu.md)

## `static class Gpu`

Device lifecycle, host bridges, kernel barriers / atomics / texture builtins.

```dream
public static class Gpu
```

## `is_available`

```dream
public static get is_available(): bool
```

## `ready`

True after a successful `try_init`.

```dream
public static get ready(): bool
```

## `check`

0 when the device is alive. A non-zero code is a pending uncaptured error or a device-lost event — pass it to `GpuError.from_code`. Call this after a submit if you need to know before the next fallible API, and after a backgrounding pause on mobile.

```dream
public static fun check(): Result<bool, GpuError>
```

## `capabilities`

Optional features and limits the device was created with. Everything reads as unavailable (`false` / `0`) until `try_init` succeeds, since capabilities are a property of the device Dream negotiated, not of the raw adapter.

```dream
public static fun capabilities(): GpuCapabilities
```

## `try_init`

Initializes the GPU device; returns `Err` with a host error code on failure. Picks a high-performance adapter when the host offers a choice.

```dream
public static async fun try_init(): Result<bool, GpuError>
```

## `try_init`

Like `try_init()`, with a cancellation token.

```dream
public static async fun try_init(token: Option<CancellationToken>): Result<bool, GpuError>
```

## `try_init`

Like `try_init()`, requesting `power` (ignored if a device is already alive).

```dream
public static async fun try_init(power: GpuPowerPreference): Result<bool, GpuError>
```

## `try_init`

Like `try_init(power)`, with a cancellation token.

```dream
public static async fun try_init( power: GpuPowerPreference, token: Option<CancellationToken> ): Result<bool, GpuError>
```

## `frame`

Yields until the next presentation/frame tick.

```dream
public static async fun frame(token: Option<CancellationToken> = Option.None): void
```

## `timestamp`

Host monotonic clock in nanoseconds (not a GPU timestamp). GPU pass timing is `GpuQuerySet` + `ComputePass.begin_timed`.

```dream
public static async fun timestamp(token: Option<CancellationToken> = Option.None): long
```

## `timestamp_period`

Nanoseconds per GPU timestamp tick. `1` when no device is alive.

```dream
public static fun timestamp_period(): float
```

## `workgroup_barrier`

Workgroup barrier (WGSL; shader-only).

```dream
public static fun workgroup_barrier(): void
```

## `storage_barrier`

Storage memory barrier (WGSL; shader-only).

```dream
public static fun storage_barrier(): void
```

## `atomic_load`

WGSL `atomicLoad` on a storage-buffer element.

```dream
public static fun atomic_load(buf: GpuBuffer<int>, i: int): int
```

## `atomic_store`

WGSL `atomicStore` to write a 32-bit integer into a storage buffer.

```dream
public static fun atomic_store(buf: GpuBuffer<int>, i: int, v: int): void
```

## `atomic_add`

WGSL `atomicAdd` to atomically add `v` to `buf[i]`; returns the old value.

```dream
public static fun atomic_add(buf: GpuBuffer<int>, i: int, v: int): int
```

## `atomic_sub`

WGSL `atomicSub` to atomically subtract `v` from `buf[i]`; returns the old value.

```dream
public static fun atomic_sub(buf: GpuBuffer<int>, i: int, v: int): int
```

## `atomic_min`

WGSL `atomicMin` to atomically compute the minimum of `v` and `buf[i]`; returns the old value.

```dream
public static fun atomic_min(buf: GpuBuffer<int>, i: int, v: int): int
```

## `atomic_max`

WGSL `atomicMax` to atomically compute the maximum of `v` and `buf[i]`; returns the old value.

```dream
public static fun atomic_max(buf: GpuBuffer<int>, i: int, v: int): int
```

## `atomic_and`

WGSL `atomicAnd` to atomically perform a bitwise AND with `v` on `buf[i]`; returns the old value.

```dream
public static fun atomic_and(buf: GpuBuffer<int>, i: int, v: int): int
```

## `atomic_or`

WGSL `atomicOr` to atomically perform a bitwise OR with `v` on `buf[i]`; returns the old value.

```dream
public static fun atomic_or(buf: GpuBuffer<int>, i: int, v: int): int
```

## `atomic_xor`

WGSL `atomicXor` to atomically perform a bitwise XOR with `v` on `buf[i]`; returns the old value.

```dream
public static fun atomic_xor(buf: GpuBuffer<int>, i: int, v: int): int
```

## `atomic_exchange`

WGSL `atomicExchange` to atomically swap `v` into `buf[i]`; returns the old value.

```dream
public static fun atomic_exchange(buf: GpuBuffer<int>, i: int, v: int): int
```
