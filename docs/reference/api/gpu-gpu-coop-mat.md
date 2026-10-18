# GpuCoopMat

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, `@gpu` and ray tracing shader functions; using them elsewhere is a compile error.

## `GpuCoopMat`

A matrix tile that a subgroup of threads holds and multiplies together on tensor hardware, for compute shaders that tile their own matrix multiplications.  Its size is fixed when the shader is built from the constant sizes passed to `zeros` or `load`. Needs `GpuFeature.CooperativeMatrices`, which `GpuTarget.Vulkan`, `Metal` and `Cuda` can provide. Metal tiles are 8 by 8, so a shader that targets `GpuTarget.Metal` must use that size. Using it with only other targets is a build error.

```dream
public struct GpuCoopMat<T>
```

## `zeros`

A `rows` by `columns` tile of zeros; both sizes must be constants.

```dream
public static extern fun zeros(rows: int, columns: int): GpuCoopMat<T>
```

## `load`

Loads a `rows` by `columns` tile whose first element is `buffer[offset]` and whose rows are `row_stride` elements apart. The sizes must be constants.

```dream
public static extern fun load(
    buffer: GpuBuffer<T>,
    offset: int,
    row_stride: int,
    rows: int,
    columns: int
): GpuCoopMat<T>
```

## `multiply_add`

`a * b + c`, where `a` is M by K, `b` K by N, and `c` and the result M by N.

```dream
public static extern fun multiply_add(
    a: GpuCoopMat<T>,
    b: GpuCoopMat<T>,
    c: GpuCoopMat<T>
): GpuCoopMat<T>
```

## `store`

Stores the tile into `buffer` starting at `buffer[offset]`, rows `row_stride` elements apart.

```dream
public extern fun store(buffer: GpuBuffer<T>, offset: int, row_stride: int): void
```
