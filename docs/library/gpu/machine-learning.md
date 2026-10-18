---
title: Machine learning
summary: Differentiate shader code automatically, use float16, and run small neural networks on tensor hardware.
icon: material/brain
api: reference/api/system-gpu.md
---

# Machine learning on the GPU

Three features help with training and running models inside shaders:

- automatic differentiation computes derivatives of `@gpu` helpers for you
- `float16` halves memory traffic
- cooperative vectors and matrices use the GPU's tensor hardware for small neural networks

## Automatic differentiation

Mark a helper `@differentiable`, and Dream's shader compiler derives its derivative. Call it
through `GpuDiff` with a `GpuDiffPair` for each parameter, holding the value and its derivative.

### Gradients with backward mode

Backward mode computes how the result changes with respect to every input at once. It is what
training needs. This program fits `weight` so that `weight * x` matches `3 * x`. Each GPU thread
computes the gradient of one sample's squared error, and the CPU takes a gradient-descent step:

```dream
import system;
import system.gpu;

@gpu
@differentiable
fun squared_error(weight: float, input: float, target: float): float {
    let error = weight * input - target;
    return error * error;
}

@compute(64, 1, 1)
fun gradient(
    @readonly inputs: GpuBuffer<float>,
    @readonly targets: GpuBuffer<float>,
    sums: GpuBuffer<float>,
    weight: float,
    n: int
): void {
    let i = global_id().x;
    if i >= n {
        return;
    }
    let w = GpuDiffPair.of(weight, 0.0f);
    let x = GpuDiffPair.constant(inputs[i]);
    let t = GpuDiffPair.constant(targets[i]);
    GpuDiff.backward3(squared_error, ref w, ref x, ref t, 1.0f);
    GpuAtomic.add_float(sums, 0, w.derivative());
}

async fun run(): Result<bool, GpuError> {
    let device = GpuDevice.create(GpuBackend.Auto).await?;

    let n = 256;
    let xs = Array.alloc<float>(n);
    let ys = Array.alloc<float>(n);
    for (let i = 0; i < n; i = i + 1) {
        xs[i] = (float)i / (float)n;
        ys[i] = 3.0f * xs[i];
    }
    let inputs = device.buffer_from(xs)?;
    let targets = device.buffer_from(ys)?;
    let sums = device.buffer_alloc<float>(1)?;

    let weight = 0.0f;
    for (let step = 0; step < 50; step = step + 1) {
        sums.write([0.0f])?;
        GpuShaders.gradient.dispatch(
            device,
            GpuDispatch.threads_1d(n),
            inputs,
            targets,
            sums,
            weight,
            n
        ).await?;
        let total = sums.read().await?;
        weight = weight - 0.01f * total[0];
    }

    System.println("learned weight: " + Math.round((double)weight * 100.0d) / 100.0d);
    return Result.Ok(true);
}

async fun main(): void {
    switch (run().await) {
        Ok(_) => {}
        Err(e) => {
            System.println("GPU error: " + e.message());
        }
    }
}
```

It prints `learned weight: 3`.

`GpuDiff.backward3` differentiates a three-parameter helper; the number in the name is the
parameter count, and `backward` and `backward2` cover one and two. Pass each pair by `ref`, then the
derivative of whatever uses the result: `1.0f` when you want the gradient of the result itself.
Afterwards each pair's `derivative()` holds the gradient for that parameter. Inputs you are not
training are `GpuDiffPair.constant`.

### Derivatives with forward mode

Forward mode computes how the result changes along one direction. Give the input you differentiate
by a derivative of `1` and the others `0`:

```dream
import system;
import system.gpu;

@gpu
@differentiable
fun bump(x: float): float {
    return x * x * x;
}

@compute(64, 1, 1)
fun slopes(@readonly xs: GpuBuffer<float>, out: GpuBuffer<float>, n: int): void {
    let i = global_id().x;
    if i < n {
        let result = GpuDiff.forward(bump, GpuDiffPair.of(xs[i], 1.0f));
        out[i] = result.derivative();
    }
}
```

`result.value()` is `bump(x)`, and `result.derivative()` is `3 * x * x`.

### What can be differentiated

Parameters and results can be `float`, `float16`, GPU vectors, and structs whose fields are all of
those. A struct parameter's gradient is a struct of the same type, with one derivative per field.

- `@no_diff` on a parameter leaves it out: it takes a plain value instead of a pair.
- `@custom_derivative(name)` replaces the automatic backward derivative with your own `@gpu`
  helper. Use it when the automatic one is slow or numerically unstable.
- Most of `GpuMath` is differentiable. `step`, `sign`, `floor`, `ceil`, `round` and `fract` are
  not, and calling them from a `@differentiable` helper is a build error.

Differentiation works on every target, WebGPU included.

## float16

`float16` stores a number in two bytes instead of four, with about three decimal digits of
precision. Shaders that compute with `float16` or read `GpuBuffer<float16>` need
`GpuFeature.Float16`:

```dream
import system;
import system.gpu;

@compute(64, 1, 1)
fun halve(values: GpuBuffer<float16>, n: int): void {
    let i = global_id().x;
    if i < n {
        values[i] = values[i] * 0.5h;
    }
}

async fun run(device: GpuDevice): Result<float16[], GpuError> {
    if !device.supports(GpuFeature.Float16) {
        return Result.Err(GpuError.UnsupportedCapability(GpuFeature.Float16));
    }
    let values = device.buffer_from([1.0h, 2.0h, 3.0h])?;
    GpuShaders.halve.dispatch(device, GpuDispatch.threads_1d(3), values, 3).await?;
    return values.read().await;
}
```

The `h` suffix writes a `float16` literal. Uploading and reading `float16` buffers works on every
device; only shader arithmetic needs the feature.

## Cooperative vectors and small neural networks

Recent GPUs have tensor units that multiply small matrices much faster than ordinary shader code.
Cooperative vectors use them to evaluate small neural networks, such as the networks in neural
materials or texture compression, inside a shader.

A layer is a weight matrix plus biases. Upload it with `GpuDevice.upload_mlp`, which stores it in
the layout the device reads fastest, and declare the shader parameter as `GpuMlpWeights<T>` with its
sizes in `@coop_shape(inputs, outputs)`:

```dream
import system;
import system.gpu;

@gpu_targets(GpuTarget.Vulkan)
@compute(32, 1, 1)
fun evaluate(
    @readonly @coop_shape(16, 32) hidden: GpuMlpWeights<float16>,
    @readonly @coop_shape(32, 4) output: GpuMlpWeights<float16>,
    @readonly @coop_shape(16) features: GpuBuffer<GpuCoopVec<float16>>,
    @coop_shape(4) results: GpuBuffer<GpuCoopVec<float16>>
): void {
    let i = global_id().x;
    let activations = GpuCoopVec.relu(hidden.apply(features[i]));
    results[i] = output.apply(activations);
}

fun upload(
    device: GpuDevice,
    weights: float16[],
    biases: float16[]
): Result<GpuMlpWeights<float16>, GpuError> {
    return device.upload_mlp(weights, biases, 16, 32);
}
```

`weights` holds `outputs` rows of `inputs` values, and `biases` one value per output. On the CPU a
`GpuBuffer<GpuCoopVec<float16>>` parameter is a plain `GpuBuffer<float16>`, with `@coop_shape`
values per element: 16 per feature vector here. Passing a layer whose sizes differ from the
parameter's `@coop_shape` fails with `GpuError.InvalidResource`.

Cooperative vectors compile for Vulkan, Direct3D 12 and CUDA, and run on devices with
`GpuFeature.CooperativeVectors`. A shader that uses them with only Metal or WebGPU as targets is a
build error.

### Cooperative matrices

`GpuCoopMat<T>` holds a matrix tile that a subgroup multiplies together. Use it to tile your own
matrix multiplications:

```dream
import system;
import system.gpu;

@gpu_targets(GpuTarget.Vulkan, GpuTarget.Metal)
@compute(32, 1, 1)
fun multiply(
    @readonly a: GpuBuffer<float16>,
    @readonly b: GpuBuffer<float16>,
    c: GpuBuffer<float16>
): void {
    let left = GpuCoopMat.load(a, 0, 8, 8, 8);
    let right = GpuCoopMat.load(b, 0, 8, 8, 8);
    let zero = GpuCoopMat<float16>.zeros(8, 8);
    let product = GpuCoopMat.multiply_add(left, right, zero);
    product.store(c, 0, 8);
}
```

`load(buffer, offset, row_stride, rows, columns)` reads a tile, `multiply_add(a, b, c)` computes
`a * b + c`, and `store` writes the result back. Sizes must be constants. Cooperative matrices need
`GpuFeature.CooperativeMatrices`: Vulkan devices with the cooperative matrix extension and
Apple-silicon Metal GPUs, where tiles are always 8 by 8.
