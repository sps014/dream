# GpuTarget

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuTarget`

A shader compilation target named in `@gpu_targets`. Each target produces one kind of GPU code, and the matching `GpuBackend` runs it.

```dream
public enum GpuTarget
```

## `Dx12`

DXIL for Direct3D 12, on Windows.

```dream
Dx12 = 1
```

## `Dx11`

DXBC for Direct3D 11, on Windows. Building it needs a Windows build machine.

```dream
Dx11 = 2
```

## `Vulkan`

SPIR-V for Vulkan, on Windows and Linux.

```dream
Vulkan = 3
```

## `Metal`

A Metal library, on macOS.

```dream
Metal = 4
```

## `Cuda`

PTX for CUDA, on Windows and Linux. Compute shaders only.

```dream
Cuda = 5
```

## `WebGpu`

WGSL for WebGPU, in browsers, Node and native programs. The default target.

```dream
WebGpu = 6
```
