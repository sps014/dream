# GpuMlpWeights

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuMlpWeights`

One uploaded neural network layer: a weight matrix plus biases, stored in the device's fastest layout for cooperative vector math.  Create one with `GpuDevice.upload_mlp` and pass it to a shader parameter, giving its sizes with `@coop_shape(inputs, outputs)`. Inside the shader, `apply` evaluates the layer. Hardware acceleration needs `GpuFeature.CooperativeVectors`, which `GpuTarget.Vulkan`, `Dx12` and `Cuda` can provide.

```dream
public class GpuMlpWeights<T: unmanaged>
```

## `inputs`

The number of values the layer reads.

```dream
public get inputs(): int
```

## `outputs`

The number of values the layer produces.

```dream
public get outputs(): int
```

## `apply`

Computes `weights * input + biases` inside a shader. `input` has `inputs` elements and the result `outputs` elements.

```dream
public extern fun apply(input: GpuCoopVec<T>): GpuCoopVec<T>
```
