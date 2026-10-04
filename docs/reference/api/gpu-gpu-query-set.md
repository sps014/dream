# GpuQuerySet

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct GpuQuerySet`

GPU timestamp query set. Requires `Gpu.capabilities().timestamp_query`. Write begin/end stamps via `ComputePass.begin_timed`, `GpuRenderTarget.timestamps`, `GpuEncoder.write_timestamp`, or `write_timestamp` on an open pass, then `read()` after submit. Values are nanoseconds (`tick * Gpu.timestamp_period()`).

```dream
public struct GpuQuerySet
```

## `id`

```dream
public get id(): int
```

## `count`

```dream
public get count(): int
```

## `timestamps`

Allocates `count` timestamp slots. Fails with `unsupported` when the device did not opt into `timestamp-query`.

```dream
public static fun timestamps(count: int): Result<GpuQuerySet, GpuError>
```

## `read`

Maps the last resolved timestamps (nanoseconds). Call after the submit that wrote them.

```dream
public fun read(): long[]
```

## `destroy`

```dream
public fun destroy(): void
```
