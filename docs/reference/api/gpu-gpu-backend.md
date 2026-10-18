# GpuBackend

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuBackend`

The graphics API a `GpuDevice` runs on.  Only backends whose shader code is in this build can be selected; `GpuShaders.backends()` lists them. Asking for any other backend returns `GpuError.BackendNotCompiled` and never falls back to a different one.

```dream
public enum GpuBackend
```

## `Auto`

Lets the host choose: the platform's native API when this build has shader code for it, otherwise WebGPU.

```dream
Auto = 0
```

## `Dx12`

Direct3D 12, on Windows.

```dream
Dx12 = 1
```

## `Dx11`

Direct3D 11, on Windows.

```dream
Dx11 = 2
```

## `Vulkan`

Vulkan, on Windows and Linux.

```dream
Vulkan = 3
```

## `Metal`

Metal, on macOS.

```dream
Metal = 4
```

## `Cuda`

CUDA, on NVIDIA GPUs under Windows and Linux. Runs compute shaders only.

```dream
Cuda = 5
```

## `WebGpu`

WebGPU: the browser's GPU API, or Dawn in native programs.

```dream
WebGpu = 6
```
