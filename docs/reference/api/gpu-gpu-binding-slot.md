# GpuBindingSlot

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuBindingSlot`

One resource parameter of a render shader, such as the texture a fragment shader samples. The shader generator exposes each as `GpuShaders.<shader>.<parameter>`; pass it to `GpuRenderPass.bind` to choose the resource the next draws use.

```dream
public class GpuBindingSlot
```

## `name`

The parameter's name.

```dream
public get name(): string
```

## `stage`

The stage of the shader that declares the parameter.

```dream
public get stage(): GpuStage
```
