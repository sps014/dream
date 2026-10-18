# GpuError

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuError`

Why a GPU operation failed.  Match on the variant to recover from specific failures, for example to try another backend after `BackendUnavailable`. `message()` gives a readable description and `code()` a stable machine code for logs.

```dream
public enum GpuError
```

## `NoDevice`

No GPU or software adapter could be found, or the host refused to create a device.

```dream
NoDevice(reason: string)
```

## `BackendNotCompiled`

The requested backend has no shader code in this build. Add its `GpuTarget` to `@gpu_targets`, or check `GpuShaders.backends()` before asking for it.

```dream
BackendNotCompiled(backend: GpuBackend)
```

## `BackendUnavailable`

The requested backend's shader code is in this build, but this machine cannot run it, for example CUDA without an NVIDIA driver. `reason` is the host's explanation.

```dream
BackendUnavailable(reason: string)
```

## `DeviceLost`

The device stopped working: a driver reset, a removed GPU, or a browser that discarded the tab. Every resource made by the device is invalid; create a new `GpuDevice` and recreate them.

```dream
DeviceLost(reason: string)
```

## `InvalidResource`

A resource was destroyed, or its size, format or usage does not fit the operation.

```dream
InvalidResource(reason: string)
```

## `WrongDevice`

A resource made by one `GpuDevice` was used with another.

```dream
WrongDevice(reason: string)
```

## `UnsupportedCapability`

The device lacks a capability that a pipeline or call needs.

```dream
UnsupportedCapability(feature: GpuFeature)
```

## `Validation`

The GPU API rejected a call, for example a pipeline whose vertex outputs do not match its fragment inputs. `reason` is the API's validation message.

```dream
Validation(reason: string)
```

## `OutOfMemory`

The GPU ran out of memory.

```dream
OutOfMemory(reason: string)
```

## `SurfaceLost`

A window surface can no longer present, usually because its window closed.

```dream
SurfaceLost(reason: string)
```

## `code`

A stable machine-readable code: `ENODEVICE`, `ENOTCOMPILED`, `EUNAVAILABLE`, `EDEVICELOST`, `EINVALID`, `EWRONGDEVICE`, `EUNSUPPORTED`, `EVALIDATION`, `ENOMEM` or `ESURFACE`.

```dream
public fun code(): string
```

## `message`

A human-readable description that includes the host's own explanation when it gave one.

```dream
public fun message(): string
```
