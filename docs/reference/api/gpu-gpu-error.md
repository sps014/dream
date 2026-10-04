# GpuError

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GpuError : Error`

Failure from GPU init / dispatch / present.

```dream
public class GpuError : Error
```

## `constructor`

Builds an error with a stable `code` string and human-readable `message`.

```dream
public constructor(code: string, message: string)
```

## `message`

Human-readable description.

```dream
public fun message(): string
```

## `code`

Stable machine-readable code (`UNAVAILABLE`, `TIMEOUT`, …).

```dream
public fun code(): string
```

## `unavailable`

```dream
public static fun unavailable(message: string): GpuError
```

## `timeout`

Operation exceeded the host timeout.

```dream
public static fun timeout(message: string): GpuError
```

## `validation`

Invalid arguments or pipeline state.

```dream
public static fun validation(message: string): GpuError
```

## `other`

Unclassified I/O or host failure.

```dream
public static fun other(message: string): GpuError
```

## `unsupported`

The request was well-formed but the adapter lacks the feature or limit it needs — a block-compressed texture format on a device without that family, for instance. Recover by picking a different resource, not by correcting the call.

```dream
public static fun unsupported(message: string): GpuError
```

## `device_lost`

The GPU device is gone (driver reset, tab discarded, thermal kill). Recover by calling `Gpu.try_init` again; previously created GPU resources are invalid.

```dream
public static fun device_lost(message: string): GpuError
```

## `cancelled`

Cooperative cancellation (`CancellationToken`).

```dream
public static fun cancelled(): GpuError
```

## `from_code`

Maps a numeric host status code to a typed `GpuError`. When the host recorded a detail string (pipeline compile failure, missing bind, wgpu validation, …), it is appended after `detail`.

```dream
public static fun from_code(code: int, detail: string): GpuError
```
