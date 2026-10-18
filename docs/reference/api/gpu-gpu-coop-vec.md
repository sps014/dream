# GpuCoopVec

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuCoopVec`

A short vector that a subgroup of threads multiplies with `GpuMlpWeights` cooperatively, on tensor hardware where the target has it. Used for small neural networks in shaders.  Its length is fixed when the shader is built: from `@coop_shape` on parameters and fields, from the constant passed to `zeros`, or from the layer that produced it. `T` is usually `float16`. `GpuTarget.Metal` and `WebGpu` cannot run cooperative vectors; using them with only those targets is a build error.

```dream
public struct GpuCoopVec<T>
```

## `zeros`

A vector of `length` zeros; `length` must be a constant.

```dream
public static extern fun zeros(length: int): GpuCoopVec<T>
```

## `relu`

Each element replaced by the larger of it and zero, the usual activation between layers.

```dream
public static extern fun relu(v: GpuCoopVec<T>): GpuCoopVec<T>
```

## `length`

The number of elements.

```dream
public extern fun length(): int
```

## `this`

Reads element `index`.

```dream
public extern fun this[index: int]: T
```

## `this`

Writes element `index`.

```dream
public extern fun this[index: int] = value: T
```
